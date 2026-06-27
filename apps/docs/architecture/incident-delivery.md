# Incident Delivery Architecture

Bella delivers incident notifications asynchronously:

```text
PostHog
  -> Bella API
  -> PostgreSQL incident_delivery_jobs
  -> bella-worker
  -> Slack API
```

The responsibilities are:

```text
incident_delivery_jobs table = durable queue
bella-worker                  = queue consumer
Railway                       = worker process host
Slack                         = external delivery destination
```

A worker is compute, not a queue. Railway keeps the worker process running,
while PostgreSQL stores the work that has not finished yet.

## Decision

Bella uses a PostgreSQL-backed job queue for incident delivery instead of
introducing SQS, RabbitMQ, Redis, or another message broker.

This is appropriate for the current product because Bella already requires
PostgreSQL, incident volume is modest, and deterministic self-hosting is more
valuable than maximizing queue throughput. The design keeps the OSS deployment
to one required stateful service while preserving durable retries and a path to
multiple workers.

This is not a claim that PostgreSQL is always the best queue. It is the
smallest architecture that currently provides the required reliability.

## Write Path

When Bella accepts a valid PostHog signal, the API:

1. Normalizes and stores the signal.
2. Creates or updates the incident.
3. Inserts a `slack.incident_opened` row into
   `incident_delivery_jobs` for a newly created incident.
4. Commits the incident and delivery job in the same PostgreSQL transaction.
5. Returns to PostHog without waiting for Slack.

Writing the incident and job in one transaction avoids a dual-write failure
where the incident commits but the notification request is lost. If the
transaction rolls back, neither record becomes visible.

The job has a unique `dedupe_key`, currently derived from the delivery type and
incident ID. Reprocessing the same source incident therefore cannot enqueue the
same root-delivery job twice.

## Worker Path

`bella-worker` is a long-running Rust process. On each polling cycle it:

1. Selects up to ten due Slack jobs.
2. Claims them atomically using `FOR UPDATE SKIP LOCKED`.
3. Loads the incident, active Slack target, and encrypted workspace bot token.
4. Decrypts the token using the environment's credential encryption key.
5. Calls the Slack API.
6. Stores the Slack channel and thread timestamp.
7. Marks the job delivered.

`SKIP LOCKED` allows multiple worker replicas to claim different jobs without
serializing all workers behind the same row lock.

A job left in `processing` for more than ten minutes becomes claimable again.
This recovers work after a worker crash or deployment interruption.

## Retry Policy

Failed jobs return to `pending` with bounded exponential backoff:

```text
attempt 1:  30 seconds
attempt 2:  60 seconds
attempt 3: 120 seconds
attempt 4: 240 seconds
attempt 5: failed permanently
```

The delay is capped for future policy changes. After the configured maximum
attempts, the job remains in `failed` with a truncated error for operator
inspection. A revoked Slack token also marks the installation as needing
attention.

## Advantages

### Fewer Required Services

PostgreSQL is already mandatory. Reusing it avoids requiring every OSS
operator to provision, secure, monitor, back up, and understand another
stateful system.

### Transactional Enqueue

The API can store the incident and enqueue its delivery atomically. A separate
broker would require a transactional outbox relay or accept a failure window
between committing PostgreSQL data and publishing a message.

### Durable Recovery

Queued jobs survive API restarts, worker restarts, and deployments. A stopped
worker does not lose incidents; delivery resumes when a worker returns.

### Independent Failure Boundaries

Slack latency and outages do not hold open PostHog webhook requests. The API
can continue ingesting incidents while workers retry Slack independently.

### Horizontal Worker Scaling

Additional worker replicas can consume from the same table. Row-level locking
prevents them from intentionally claiming the same available job
simultaneously.

### Operational Visibility

Operators can inspect pending, processing, delivered, and failed jobs with
ordinary SQL and correlate each job with its organization and incident.

### Deterministic Local Development

The local, self-hosted, QA, and production paths use the same queue behavior.
Developers do not need an in-memory substitute that behaves differently from a
cloud broker.

## Trade-Offs

### PostgreSQL Carries Queue Load

Polling, claims, retries, and job retention add database reads and writes. At
high sustained throughput this can compete with customer-facing queries for
connections, I/O, locks, and storage.

The current worker polls on an interval, claims ten jobs, and processes that
batch sequentially. This favors simplicity over minimum latency and maximum
throughput.

### No Broker-Native Features

The implementation does not currently provide broker-native capabilities such
as visibility dashboards, dead-letter queues, push-based consumption,
per-message retention policies, queue-level autoscaling metrics, or managed
cross-region replication.

Failed rows provide the basis for a dead-letter workflow, but operator tooling
for replaying them still needs to be built.

### At-Least-Once Delivery

The worker and Slack cannot participate in one distributed transaction.
Consider this sequence:

1. Slack accepts a message.
2. The worker crashes before storing the returned thread timestamp.
3. The stale job becomes claimable.
4. A worker sends the message again.

The stored thread timestamp prevents normal retries from creating another root
message, but it cannot close the crash window after Slack accepts the message
and before PostgreSQL commits the result. Delivery is therefore at-least-once,
not exactly-once.

Exactly-once delivery cannot be guaranteed merely by replacing PostgreSQL with
SQS. The external Slack side effect would still sit outside the queue's
transaction boundary.

### Polling Latency

New jobs wait until the next worker poll. Lower intervals reduce latency but
increase idle database traffic. QA uses a shorter interval for feedback;
production can use a longer interval until incident volume requires a different
balance.

### Database Availability Is Shared

If PostgreSQL is unavailable, both incident ingestion and queue consumption
stop. A separate broker could isolate queue availability, although the API
would still need a safe strategy for publishing messages after database
commits.

## Why Not Send From the API?

Sending to Slack directly in the PostHog request would make the design simpler
only in the happy path. It would also:

- Couple webhook latency to Slack latency.
- Turn a Slack outage into an ingestion outage.
- Make retries depend on PostHog retry behavior.
- Increase the chance of duplicate Slack messages.
- Lose unfinished work when an API process exits.
- Consume API capacity with external delivery work.

The queue and worker let the API acknowledge durable ingestion independently
of notification delivery.

## Why Not SQS Yet?

An SQS design would still require a worker:

```text
Bella API -> SQS -> bella-worker -> Slack
```

SQS would replace the PostgreSQL queue, not the consumer. It becomes useful
when managed queue capabilities justify the additional cloud dependency and
the more complicated OSS deployment model.

Moving directly from the API transaction to SQS would introduce a dual-write
problem. The safe migration would retain a PostgreSQL outbox record:

```text
API transaction
  -> incident + outbox row
outbox relay
  -> SQS
worker
  -> Slack
```

## Revisit Criteria

Re-evaluate PostgreSQL as the queue when measurements show one or more of:

- Queue traffic materially affects API database latency.
- Delivery backlog regularly exceeds the incident response objective.
- Worker throughput requires high replica counts or large claim batches.
- Polling creates unacceptable database load.
- Independent queue availability or cross-region delivery is required.
- Managed dead-letter queues, retention, replay, or autoscaling become
  operational requirements.
- Multiple high-volume delivery types need independent throughput and failure
  isolation.

The migration decision should be driven by observed throughput, backlog age,
database load, and recovery requirements rather than anticipated scale alone.

## Operational Signals

At minimum, production monitoring should track:

- Number of pending, processing, and failed jobs.
- Age of the oldest pending job.
- Delivery success and failure counts.
- Attempts per job and retry delay.
- Worker poll and processing duration.
- Stale processing jobs reclaimed after ten minutes.
- Slack rate-limit and token-revocation errors.
- Worker process availability.

These signals matter regardless of whether the durable queue is PostgreSQL,
SQS, or another broker.
