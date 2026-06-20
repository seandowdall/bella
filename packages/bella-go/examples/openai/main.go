package main

import (
	"context"
	"log"

	bella "github.com/seandowdall/bella/packages/bella-go"
)

type chatCompletion struct {
	Text         string
	Model        string
	InputTokens  int64
	OutputTokens int64
}

func main() {
	ctx := context.Background()

	server, ok, err := bella.NewServerFromEnv()
	if err != nil {
		log.Fatal(err)
	}

	if !ok {
		completion, err := createChatCompletion(ctx)
		if err != nil {
			log.Fatal(err)
		}
		log.Println(completion.Text)
		return
	}

	completion, err := bella.TrackLlmCall(ctx, server, bella.TrackLlmCallOptions[chatCompletion]{
		Operation: "chat.completions.create",
		Call: func(ctx context.Context) (chatCompletion, error) {
			return createChatCompletion(ctx)
		},
		ModelFromResult: func(result chatCompletion) string {
			return result.Model
		},
		UsageFromResult: func(result chatCompletion) *bella.Usage {
			totalTokens := result.InputTokens + result.OutputTokens
			return &bella.Usage{
				InputTokens:  &result.InputTokens,
				OutputTokens: &result.OutputTokens,
				TotalTokens:  &totalTokens,
			}
		},
		Metadata: map[string]any{
			"service": "example",
		},
	})
	if err != nil {
		log.Fatal(err)
	}

	log.Println(completion.Text)
}

func createChatCompletion(context.Context) (chatCompletion, error) {
	return chatCompletion{
		Text:         "provider response",
		Model:        "gpt-4.1-mini",
		InputTokens:  10,
		OutputTokens: 20,
	}, nil
}
