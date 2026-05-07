// Package jsonrpc provides JSON-RPC 2.0 request handling
package jsonrpc

import (
	"context"
	"encoding/json"
	"log"
)

// Handler handles JSON-RPC method calls
type Handler interface {
	Handle(ctx context.Context, method string, params json.RawMessage) (any, *Error)
}

// HandlerFunc is a function that implements Handler
type HandlerFunc func(ctx context.Context, method string, params json.RawMessage) (any, *Error)

// Handle implements Handler
func (f HandlerFunc) Handle(ctx context.Context, method string, params json.RawMessage) (any, *Error) {
	return f(ctx, method, params)
}

// Router routes JSON-RPC requests to handlers
type Router struct {
	methods map[string]Handler
}

// NewRouter creates a new router
func NewRouter() *Router {
	return &Router{
		methods: make(map[string]Handler),
	}
}

// Register registers a handler for a method
func (r *Router) Register(method string, handler Handler) {
	r.methods[method] = handler
}

// RegisterFunc registers a handler function for a method
func (r *Router) RegisterFunc(method string, handler HandlerFunc) {
	r.Register(method, handler)
}

// Route routes a request to the appropriate handler
func (r *Router) Route(ctx context.Context, method string, params json.RawMessage) (any, *Error) {
	handler, ok := r.methods[method]
	if !ok {
		return nil, NewMethodNotFoundError(method)
	}
	return handler.Handle(ctx, method, params)
}

// Process processes a raw JSON-RPC request and returns a response
func (r *Router) Process(ctx context.Context, data []byte) *Response {
	// Parse request
	var req Request
	if err := json.Unmarshal(data, &req); err != nil {
		return &Response{
			JSONRPC: "2.0",
			Error:   NewParseError(err.Error()),
		}
	}

	// Validate JSON-RPC version
	if req.JSONRPC != "2.0" {
		return &Response{
			JSONRPC: "2.0",
			ID:      req.ID,
			Error:   NewInvalidRequestError("invalid jsonrpc version"),
		}
	}

	// Route to handler
	result, rpcErr := r.Route(ctx, req.Method, req.Params)
	if rpcErr != nil {
		return &Response{
			JSONRPC: "2.0",
			ID:      req.ID,
			Error:   rpcErr,
		}
	}

	// Marshal result
	resultData, err := json.Marshal(result)
	if err != nil {
		log.Printf("Failed to marshal result: %v", err)
		return &Response{
			JSONRPC: "2.0",
			ID:      req.ID,
			Error:   NewInternalError("failed to marshal result"),
		}
	}

	return &Response{
		JSONRPC: "2.0",
		ID:      req.ID,
		Result:  resultData,
	}
}

// ProcessNotification processes a notification (no response needed)
func (r *Router) ProcessNotification(ctx context.Context, data []byte) {
	var req Request
	if err := json.Unmarshal(data, &req); err != nil {
		log.Printf("Failed to parse notification: %v", err)
		return
	}

	if req.JSONRPC != "2.0" {
		log.Printf("Invalid jsonrpc version in notification")
		return
	}

	// Route to handler but ignore result
	_, _ = r.Route(ctx, req.Method, req.Params)
}

// HasMethod checks if a method is registered
func (r *Router) HasMethod(method string) bool {
	_, ok := r.methods[method]
	return ok
}

// Methods returns all registered method names
func (r *Router) Methods() []string {
	methods := make([]string, 0, len(r.methods))
	for method := range r.methods {
		methods = append(methods, method)
	}
	return methods
}
