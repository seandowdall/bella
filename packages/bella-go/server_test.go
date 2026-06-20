package bella

import (
	"context"
	"encoding/json"
	"errors"
	"net/http"
	"net/http/httptest"
	"testing"
)

func TestTrackLlmCallRecordsSucceededEvent(t *testing.T) {
	var payload map[string]any
	api := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if err := json.NewDecoder(r.Body).Decode(&payload); err != nil {
			t.Fatalf("decode body: %v", err)
		}
		w.WriteHeader(http.StatusCreated)
		_, _ = w.Write([]byte(`{"event_id":"llm_test","accepted":true}`))
	}))
	defer api.Close()

	server, err := NewServer(ServerOptions{
		ClientOptions: ClientOptions{
			APIKey:         "bella_key",
			BaseURL:        api.URL,
			OrganizationID: "org_123",
			HTTPClient:     api.Client(),
		},
		DefaultProviderAccountID: "account_123",
		DefaultProvider:          "openai",
	})
	if err != nil {
		t.Fatalf("NewServer: %v", err)
	}

	inputTokens := int64(10)
	result, err := TrackLlmCall(context.Background(), server, TrackLlmCallOptions[string]{
		EventID:   "llm_test",
		Model:     "gpt-4.1-mini",
		Operation: "chat.completions.create",
		Metadata:  map[string]any{"service": "worker"},
		Call: func(context.Context) (string, error) {
			return "ok", nil
		},
		UsageFromResult: func(string) *Usage {
			return &Usage{InputTokens: &inputTokens}
		},
	})
	if err != nil {
		t.Fatalf("TrackLlmCall: %v", err)
	}
	if result != "ok" {
		t.Fatalf("result = %q", result)
	}

	assertEqual(t, payload["event_id"], "llm_test")
	assertEqual(t, payload["provider_account_id"], "account_123")
	assertEqual(t, payload["provider"], "openai")
	assertEqual(t, payload["model"], "gpt-4.1-mini")
	assertEqual(t, payload["operation"], "chat.completions.create")
	assertEqual(t, payload["status"], string(UsageStatusSucceeded))
	usage := payload["usage"].(map[string]any)
	assertEqual(t, usage["input_tokens"], float64(inputTokens))
}

func TestTrackLlmCallFailsOpenOnIngestionError(t *testing.T) {
	api := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		http.Error(w, "unavailable", http.StatusServiceUnavailable)
	}))
	defer api.Close()

	var observedEvent UsageEvent
	server, err := NewServer(ServerOptions{
		ClientOptions: ClientOptions{
			APIKey:         "bella_key",
			BaseURL:        api.URL,
			OrganizationID: "org_123",
			HTTPClient:     api.Client(),
		},
		DefaultProviderAccountID: "account_123",
		DefaultProvider:          "openai",
		OnIngestionError: func(_ error, event UsageEvent) {
			observedEvent = event
		},
	})
	if err != nil {
		t.Fatalf("NewServer: %v", err)
	}

	result, err := TrackLlmCall(context.Background(), server, TrackLlmCallOptions[string]{
		EventID: "llm_test",
		Call: func(context.Context) (string, error) {
			return "provider result", nil
		},
	})
	if err != nil {
		t.Fatalf("TrackLlmCall: %v", err)
	}
	if result != "provider result" {
		t.Fatalf("result = %q", result)
	}
	if observedEvent.EventID != "llm_test" {
		t.Fatalf("observed event = %+v", observedEvent)
	}
}

func TestTrackLlmCallCanFailClosedOnIngestionError(t *testing.T) {
	api := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		http.Error(w, "unavailable", http.StatusServiceUnavailable)
	}))
	defer api.Close()

	failOpen := false
	server, err := NewServer(ServerOptions{
		ClientOptions: ClientOptions{
			APIKey:         "bella_key",
			BaseURL:        api.URL,
			OrganizationID: "org_123",
			HTTPClient:     api.Client(),
		},
		DefaultProviderAccountID: "account_123",
		DefaultProvider:          "openai",
		FailOpen:                 &failOpen,
	})
	if err != nil {
		t.Fatalf("NewServer: %v", err)
	}

	_, err = TrackLlmCall(context.Background(), server, TrackLlmCallOptions[string]{
		EventID: "llm_test",
		Call: func(context.Context) (string, error) {
			return "provider result", nil
		},
	})
	var apiError *APIError
	if !errors.As(err, &apiError) {
		t.Fatalf("error = %v, want APIError", err)
	}
}

func TestTrackLlmCallRecordsFailedEventAndRethrowsCallError(t *testing.T) {
	var payload map[string]any
	api := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if err := json.NewDecoder(r.Body).Decode(&payload); err != nil {
			t.Fatalf("decode body: %v", err)
		}
		w.WriteHeader(http.StatusCreated)
		_, _ = w.Write([]byte(`{"event_id":"llm_test","accepted":true}`))
	}))
	defer api.Close()

	server, err := NewServer(ServerOptions{
		ClientOptions: ClientOptions{
			APIKey:         "bella_key",
			BaseURL:        api.URL,
			OrganizationID: "org_123",
			HTTPClient:     api.Client(),
		},
		DefaultProviderAccountID: "account_123",
		DefaultProvider:          "openai",
		CaptureErrorMessage:      true,
	})
	if err != nil {
		t.Fatalf("NewServer: %v", err)
	}

	callErr := errors.New("provider failed")
	_, err = TrackLlmCall(context.Background(), server, TrackLlmCallOptions[string]{
		EventID: "llm_test",
		Call: func(context.Context) (string, error) {
			return "", callErr
		},
	})
	if !errors.Is(err, callErr) {
		t.Fatalf("error = %v, want call error", err)
	}

	assertEqual(t, payload["status"], string(UsageStatusFailed))
	assertEqual(t, payload["error_message"], "provider failed")
}

func TestTrackLlmCallUsesCustomErrorMessageExtractor(t *testing.T) {
	var payload map[string]any
	api := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if err := json.NewDecoder(r.Body).Decode(&payload); err != nil {
			t.Fatalf("decode body: %v", err)
		}
		w.WriteHeader(http.StatusCreated)
		_, _ = w.Write([]byte(`{"event_id":"llm_test","accepted":true}`))
	}))
	defer api.Close()

	server, err := NewServer(ServerOptions{
		ClientOptions: ClientOptions{
			APIKey:         "bella_key",
			BaseURL:        api.URL,
			OrganizationID: "org_123",
			HTTPClient:     api.Client(),
		},
		DefaultProviderAccountID: "account_123",
		DefaultProvider:          "openai",
		ErrorMessageFromError: func(error) string {
			return "redacted provider error"
		},
	})
	if err != nil {
		t.Fatalf("NewServer: %v", err)
	}

	_, err = TrackLlmCall(context.Background(), server, TrackLlmCallOptions[string]{
		EventID: "llm_test",
		Call: func(context.Context) (string, error) {
			return "", errors.New("provider failed with sensitive details")
		},
	})
	if err == nil {
		t.Fatal("expected call error")
	}

	assertEqual(t, payload["error_message"], "redacted provider error")
}

func TestNewServerFromEnv(t *testing.T) {
	t.Setenv("BELLA_API_KEY", "bella_key")
	t.Setenv("BELLA_API_URL", "http://bella.example")
	t.Setenv("BELLA_ORGANIZATION_ID", "org_123")
	t.Setenv("BELLA_PROVIDER_ACCOUNT_ID", "account_123")
	t.Setenv("BELLA_PROVIDER", "anthropic")
	t.Setenv("BELLA_SDK_FAIL_OPEN", "false")
	t.Setenv("BELLA_SDK_CAPTURE_ERROR_MESSAGE", "true")

	server, ok, err := NewServerFromEnv()
	if err != nil {
		t.Fatalf("NewServerFromEnv: %v", err)
	}
	if !ok || server == nil {
		t.Fatal("expected configured server")
	}
	if server.defaultProvider != "anthropic" {
		t.Fatalf("provider = %q", server.defaultProvider)
	}
	if server.failOpen {
		t.Fatal("expected failOpen false")
	}
	if !server.captureErrorMessage {
		t.Fatal("expected captureErrorMessage true")
	}
}

func TestNewServerFromEnvReturnsFalseWhenMissingRequiredEnv(t *testing.T) {
	server, ok, err := NewServerFromEnv()
	if err != nil {
		t.Fatalf("NewServerFromEnv: %v", err)
	}
	if ok || server != nil {
		t.Fatalf("server = %+v, ok = %v", server, ok)
	}
}
