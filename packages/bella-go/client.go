package bella

import (
	"bytes"
	"context"
	"crypto/rand"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"strings"
	"time"
)

const defaultBaseURL = "http://127.0.0.1:3000"

type UsageStatus string

const (
	UsageStatusSucceeded UsageStatus = "succeeded"
	UsageStatusFailed    UsageStatus = "failed"
)

type Usage struct {
	InputTokens  *int64
	OutputTokens *int64
	TotalTokens  *int64
}

type Cost struct {
	AmountMicros int64
	Currency     string
}

type UsageEvent struct {
	EventID           string
	ProviderAccountID string
	Provider          string
	Model             string
	Operation         string
	Status            UsageStatus
	StartedAt         time.Time
	EndedAt           time.Time
	Usage             *Usage
	Cost              *Cost
	Metadata          map[string]any
	ErrorMessage      string
}

type UsageEventResponse struct {
	EventID  string `json:"event_id"`
	Accepted bool   `json:"accepted"`
}

type ClientOptions struct {
	APIKey         string
	BaseURL        string
	OrganizationID string
	HTTPClient     *http.Client
}

type Client struct {
	apiKey         string
	baseURL        string
	organizationID string
	httpClient     *http.Client
}

type APIError struct {
	StatusCode int
	Body       string
}

func (e *APIError) Error() string {
	if e.Body == "" {
		return fmt.Sprintf("bella API request failed with HTTP %d", e.StatusCode)
	}
	return fmt.Sprintf("bella API request failed with HTTP %d: %s", e.StatusCode, e.Body)
}

func NewClient(options ClientOptions) (*Client, error) {
	if strings.TrimSpace(options.APIKey) == "" {
		return nil, fmt.Errorf("bella api key is required")
	}
	if strings.TrimSpace(options.OrganizationID) == "" {
		return nil, fmt.Errorf("bella organization id is required")
	}

	baseURL := strings.TrimRight(strings.TrimSpace(options.BaseURL), "/")
	if baseURL == "" {
		baseURL = defaultBaseURL
	}

	httpClient := options.HTTPClient
	if httpClient == nil {
		httpClient = http.DefaultClient
	}

	return &Client{
		apiKey:         options.APIKey,
		baseURL:        baseURL,
		organizationID: options.OrganizationID,
		httpClient:     httpClient,
	}, nil
}

func (c *Client) RecordUsageEvent(ctx context.Context, event UsageEvent) (*UsageEventResponse, error) {
	payload, err := json.Marshal(toWireUsageEvent(event))
	if err != nil {
		return nil, fmt.Errorf("marshal bella usage event: %w", err)
	}

	url := fmt.Sprintf("%s/v1/organizations/%s/sdk/usage-events", c.baseURL, c.organizationID)
	request, err := http.NewRequestWithContext(ctx, http.MethodPost, url, bytes.NewReader(payload))
	if err != nil {
		return nil, fmt.Errorf("create bella usage event request: %w", err)
	}
	request.Header.Set("Authorization", "Bearer "+c.apiKey)
	request.Header.Set("Content-Type", "application/json")

	response, err := c.httpClient.Do(request)
	if err != nil {
		return nil, fmt.Errorf("send bella usage event: %w", err)
	}
	defer response.Body.Close()

	body, err := io.ReadAll(response.Body)
	if err != nil {
		return nil, fmt.Errorf("read bella usage event response: %w", err)
	}
	if response.StatusCode < 200 || response.StatusCode > 299 {
		return nil, &APIError{StatusCode: response.StatusCode, Body: string(body)}
	}

	var decoded UsageEventResponse
	if err := json.Unmarshal(body, &decoded); err != nil {
		return nil, fmt.Errorf("decode bella usage event response: %w", err)
	}
	return &decoded, nil
}

func CreateEventID(prefix string) string {
	if prefix == "" {
		prefix = "evt"
	}

	var randomBytes [16]byte
	if _, err := rand.Read(randomBytes[:]); err != nil {
		return fmt.Sprintf("%s_%d", prefix, time.Now().UnixNano())
	}
	return prefix + "_" + hex.EncodeToString(randomBytes[:])
}

type wireUsageEvent struct {
	EventID           string         `json:"event_id"`
	ProviderAccountID string         `json:"provider_account_id"`
	Provider          string         `json:"provider"`
	Model             string         `json:"model,omitempty"`
	Operation         string         `json:"operation,omitempty"`
	Status            UsageStatus    `json:"status"`
	StartedAt         string         `json:"started_at"`
	EndedAt           string         `json:"ended_at"`
	Usage             *wireUsage     `json:"usage,omitempty"`
	Cost              *wireCost      `json:"cost,omitempty"`
	Metadata          map[string]any `json:"metadata,omitempty"`
	ErrorMessage      string         `json:"error_message,omitempty"`
}

type wireUsage struct {
	InputTokens  *int64 `json:"input_tokens,omitempty"`
	OutputTokens *int64 `json:"output_tokens,omitempty"`
	TotalTokens  *int64 `json:"total_tokens,omitempty"`
}

type wireCost struct {
	AmountMicros int64  `json:"amount_micros"`
	Currency     string `json:"currency,omitempty"`
}

func toWireUsageEvent(event UsageEvent) wireUsageEvent {
	var usage *wireUsage
	if event.Usage != nil {
		usage = &wireUsage{
			InputTokens:  event.Usage.InputTokens,
			OutputTokens: event.Usage.OutputTokens,
			TotalTokens:  event.Usage.TotalTokens,
		}
	}

	var cost *wireCost
	if event.Cost != nil {
		cost = &wireCost{
			AmountMicros: event.Cost.AmountMicros,
			Currency:     event.Cost.Currency,
		}
	}

	return wireUsageEvent{
		EventID:           event.EventID,
		ProviderAccountID: event.ProviderAccountID,
		Provider:          event.Provider,
		Model:             event.Model,
		Operation:         event.Operation,
		Status:            event.Status,
		StartedAt:         event.StartedAt.UTC().Format(time.RFC3339Nano),
		EndedAt:           event.EndedAt.UTC().Format(time.RFC3339Nano),
		Usage:             usage,
		Cost:              cost,
		Metadata:          event.Metadata,
		ErrorMessage:      event.ErrorMessage,
	}
}
