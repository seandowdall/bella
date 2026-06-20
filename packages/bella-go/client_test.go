package bella

import (
	"context"
	"encoding/json"
	"errors"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
	"time"
)

func TestRecordUsageEventPostsWirePayload(t *testing.T) {
	startedAt := time.Date(2026, 6, 20, 12, 0, 0, 123, time.UTC)
	endedAt := startedAt.Add(2 * time.Second)
	inputTokens := int64(10)
	outputTokens := int64(20)
	totalTokens := int64(30)

	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.Method != http.MethodPost {
			t.Fatalf("method = %s, want POST", r.Method)
		}
		if r.URL.Path != "/v1/organizations/org_123/sdk/usage-events" {
			t.Fatalf("path = %s", r.URL.Path)
		}
		if got := r.Header.Get("Authorization"); got != "Bearer bella_key" {
			t.Fatalf("authorization = %q", got)
		}
		if got := r.Header.Get("Content-Type"); !strings.HasPrefix(got, "application/json") {
			t.Fatalf("content-type = %q", got)
		}

		var payload map[string]any
		if err := json.NewDecoder(r.Body).Decode(&payload); err != nil {
			t.Fatalf("decode body: %v", err)
		}

		assertEqual(t, payload["event_id"], "llm_event")
		assertEqual(t, payload["provider_account_id"], "account_123")
		assertEqual(t, payload["provider"], "openai")
		assertEqual(t, payload["model"], "gpt-4.1-mini")
		assertEqual(t, payload["operation"], "chat.completions.create")
		assertEqual(t, payload["status"], string(UsageStatusSucceeded))
		assertEqual(t, payload["started_at"], startedAt.Format(time.RFC3339Nano))
		assertEqual(t, payload["ended_at"], endedAt.Format(time.RFC3339Nano))

		usage := payload["usage"].(map[string]any)
		assertEqual(t, usage["input_tokens"], float64(inputTokens))
		assertEqual(t, usage["output_tokens"], float64(outputTokens))
		assertEqual(t, usage["total_tokens"], float64(totalTokens))

		cost := payload["cost"].(map[string]any)
		assertEqual(t, cost["amount_micros"], float64(12345))
		assertEqual(t, cost["currency"], "usd")

		metadata := payload["metadata"].(map[string]any)
		assertEqual(t, metadata["service"], "billing")

		w.WriteHeader(http.StatusCreated)
		_, _ = w.Write([]byte(`{"event_id":"llm_event","accepted":true}`))
	}))
	defer server.Close()

	client, err := NewClient(ClientOptions{
		APIKey:         "bella_key",
		BaseURL:        server.URL + "/",
		OrganizationID: "org_123",
		HTTPClient:     server.Client(),
	})
	if err != nil {
		t.Fatalf("NewClient: %v", err)
	}

	response, err := client.RecordUsageEvent(context.Background(), UsageEvent{
		EventID:           "llm_event",
		ProviderAccountID: "account_123",
		Provider:          "openai",
		Model:             "gpt-4.1-mini",
		Operation:         "chat.completions.create",
		Status:            UsageStatusSucceeded,
		StartedAt:         startedAt,
		EndedAt:           endedAt,
		Usage: &Usage{
			InputTokens:  &inputTokens,
			OutputTokens: &outputTokens,
			TotalTokens:  &totalTokens,
		},
		Cost: &Cost{
			AmountMicros: 12345,
			Currency:     "usd",
		},
		Metadata: map[string]any{"service": "billing"},
	})
	if err != nil {
		t.Fatalf("RecordUsageEvent: %v", err)
	}
	if response.EventID != "llm_event" || !response.Accepted {
		t.Fatalf("response = %+v", response)
	}
}

func TestRecordUsageEventReturnsAPIError(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		http.Error(w, "bad provider", http.StatusBadRequest)
	}))
	defer server.Close()

	client, err := NewClient(ClientOptions{
		APIKey:         "bella_key",
		BaseURL:        server.URL,
		OrganizationID: "org_123",
		HTTPClient:     server.Client(),
	})
	if err != nil {
		t.Fatalf("NewClient: %v", err)
	}

	_, err = client.RecordUsageEvent(context.Background(), UsageEvent{
		EventID:           "llm_event",
		ProviderAccountID: "account_123",
		Provider:          "openai",
		Status:            UsageStatusSucceeded,
		StartedAt:         time.Now(),
		EndedAt:           time.Now(),
	})
	var apiError *APIError
	if !errors.As(err, &apiError) {
		t.Fatalf("error = %v, want APIError", err)
	}
	if apiError.StatusCode != http.StatusBadRequest {
		t.Fatalf("status = %d", apiError.StatusCode)
	}
	if !strings.Contains(apiError.Body, "bad provider") {
		t.Fatalf("body = %q", apiError.Body)
	}
}

func TestNewClientValidatesRequiredOptions(t *testing.T) {
	if _, err := NewClient(ClientOptions{OrganizationID: "org_123"}); err == nil {
		t.Fatal("expected missing api key error")
	}
	if _, err := NewClient(ClientOptions{APIKey: "bella_key"}); err == nil {
		t.Fatal("expected missing organization id error")
	}
}

func assertEqual(t *testing.T, got, want any) {
	t.Helper()
	if got != want {
		t.Fatalf("got %v, want %v", got, want)
	}
}
