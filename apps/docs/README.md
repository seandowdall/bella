# Bella Docs

Documentation will live here.

## Prerequisites

- [OrbStack](https://orbstack.dev/)
- [just](https://github.com/casey/just)

## Initial Setup

Install `just` if it is not already available:

```sh
brew install just
```

Create the local environment file from the example:

```sh
test -f .env || cp .env.example .env
```

From the repository root, start the Docker services and Bella API:

```sh
just dev
```
