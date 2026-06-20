package bella

import (
	"context"
	"fmt"
	"os"
	"strings"
	"time"
)

const defaultOperation = "llm.call"

type ServerOptions struct {
	ClientOptions
	DefaultProviderAccountID string
	DefaultProvider          string
	FailOpen                 *bool
	CaptureErrorMessage      bool
	ErrorMessageFromError    func(error) string
	OnIngestionError         func(error, UsageEvent)
}

type Server struct {
	client                   *Client
	defaultProviderAccountID string
	defaultProvider          string
	failOpen                 bool
	captureErrorMessage      bool
	errorMessageFromError    func(error) string
	onIngestionError         func(error, UsageEvent)
}

type TrackLlmCallOptions[T any] struct {
	ProviderAccountID     string
	Provider              string
	Model                 string
	Operation             string
	EventID               string
	Metadata              map[string]any
	Call                  func(context.Context) (T, error)
	UsageFromResult       func(T) *Usage
	CostFromResult        func(T) *Cost
	ModelFromResult       func(T) string
	CaptureErrorMessage   *bool
	ErrorMessageFromError func(error) string
}

func NewServer(options ServerOptions) (*Server, error) {
	client, err := NewClient(options.ClientOptions)
	if err != nil {
		return nil, err
	}

	failOpen := true
	if options.FailOpen != nil {
		failOpen = *options.FailOpen
	}

	return &Server{
		client:                   client,
		defaultProviderAccountID: options.DefaultProviderAccountID,
		defaultProvider:          options.DefaultProvider,
		failOpen:                 failOpen,
		captureErrorMessage:      options.CaptureErrorMessage,
		errorMessageFromError:    options.ErrorMessageFromError,
		onIngestionError:         options.OnIngestionError,
	}, nil
}

func NewServerFromEnv() (*Server, bool, error) {
	apiKey := os.Getenv("BELLA_API_KEY")
	organizationID := os.Getenv("BELLA_ORGANIZATION_ID")
	if apiKey == "" || organizationID == "" {
		return nil, false, nil
	}

	failOpen := os.Getenv("BELLA_SDK_FAIL_OPEN") != "false"
	baseURL := os.Getenv("BELLA_API_URL")
	if baseURL == "" {
		baseURL = os.Getenv("BELLA_PUBLIC_API_URL")
	}

	provider := os.Getenv("BELLA_PROVIDER")
	if provider == "" {
		provider = "openai"
	}

	server, err := NewServer(ServerOptions{
		ClientOptions: ClientOptions{
			APIKey:         apiKey,
			BaseURL:        baseURL,
			OrganizationID: organizationID,
		},
		DefaultProviderAccountID: os.Getenv("BELLA_PROVIDER_ACCOUNT_ID"),
		DefaultProvider:          provider,
		FailOpen:                 &failOpen,
		CaptureErrorMessage:      os.Getenv("BELLA_SDK_CAPTURE_ERROR_MESSAGE") == "true",
	})
	if err != nil {
		return nil, false, err
	}
	return server, true, nil
}

func TrackLlmCall[T any](ctx context.Context, server *Server, options TrackLlmCallOptions[T]) (T, error) {
	var zero T
	if server == nil {
		return zero, fmt.Errorf("bella server is required")
	}
	if options.Call == nil {
		return zero, fmt.Errorf("bella call is required")
	}

	providerAccountID := firstNonEmpty(options.ProviderAccountID, server.defaultProviderAccountID)
	if providerAccountID == "" {
		return zero, fmt.Errorf("bella provider account id is required")
	}

	provider := firstNonEmpty(options.Provider, server.defaultProvider)
	if provider == "" {
		return zero, fmt.Errorf("bella provider is required")
	}

	eventID := options.EventID
	if eventID == "" {
		eventID = CreateEventID("llm")
	}

	operation := options.Operation
	if operation == "" {
		operation = defaultOperation
	}

	startedAt := time.Now()
	result, callErr := options.Call(ctx)
	endedAt := time.Now()
	if callErr != nil {
		event := UsageEvent{
			EventID:           eventID,
			ProviderAccountID: providerAccountID,
			Provider:          provider,
			Model:             options.Model,
			Operation:         operation,
			Status:            UsageStatusFailed,
			StartedAt:         startedAt,
			EndedAt:           endedAt,
			Metadata:          options.Metadata,
			ErrorMessage:      capturedErrorMessage(callErr, errorMessageExtractor(server, options), captureErrors(server, options)),
		}
		if err := server.safeRecordUsageEvent(ctx, event); err != nil {
			return zero, err
		}
		return zero, callErr
	}

	model := options.Model
	if model == "" && options.ModelFromResult != nil {
		model = options.ModelFromResult(result)
	}

	var usage *Usage
	if options.UsageFromResult != nil {
		usage = options.UsageFromResult(result)
	}

	var cost *Cost
	if options.CostFromResult != nil {
		cost = options.CostFromResult(result)
	}

	event := UsageEvent{
		EventID:           eventID,
		ProviderAccountID: providerAccountID,
		Provider:          provider,
		Model:             model,
		Operation:         operation,
		Status:            UsageStatusSucceeded,
		StartedAt:         startedAt,
		EndedAt:           endedAt,
		Usage:             usage,
		Cost:              cost,
		Metadata:          options.Metadata,
	}
	if err := server.safeRecordUsageEvent(ctx, event); err != nil {
		return zero, err
	}
	return result, nil
}

func (s *Server) RecordUsageEvent(ctx context.Context, event UsageEvent) (*UsageEventResponse, error) {
	return s.client.RecordUsageEvent(ctx, event)
}

func (s *Server) safeRecordUsageEvent(ctx context.Context, event UsageEvent) error {
	_, err := s.client.RecordUsageEvent(ctx, event)
	if err == nil {
		return nil
	}
	if s.onIngestionError != nil {
		s.onIngestionError(err, event)
	}
	if s.failOpen {
		return nil
	}
	return err
}

func captureErrors[T any](server *Server, options TrackLlmCallOptions[T]) bool {
	if options.ErrorMessageFromError != nil || server.errorMessageFromError != nil {
		return true
	}
	if options.CaptureErrorMessage != nil {
		return *options.CaptureErrorMessage
	}
	return server.captureErrorMessage
}

func errorMessageExtractor[T any](server *Server, options TrackLlmCallOptions[T]) func(error) string {
	if options.ErrorMessageFromError != nil {
		return options.ErrorMessageFromError
	}
	return server.errorMessageFromError
}

func capturedErrorMessage(err error, extract func(error) string, capture bool) string {
	if err == nil {
		return ""
	}
	if extract == nil && !capture {
		return ""
	}

	message := err.Error()
	if extract != nil {
		message = extract(err)
	}
	message = strings.TrimSpace(message)
	if len(message) > 1000 {
		return message[:1000]
	}
	return message
}

func firstNonEmpty(values ...string) string {
	for _, value := range values {
		if value != "" {
			return value
		}
	}
	return ""
}
