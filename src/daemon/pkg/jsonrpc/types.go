// Package jsonrpc implements JSON-RPC 2.0 protocol
package jsonrpc

import (
	"encoding/json"
	"fmt"
)

// Request represents a JSON-RPC 2.0 request
// https://www.jsonrpc.org/specification#request_object
type Request struct {
	JSONRPC string          `json:"jsonrpc"`
	Method  string          `json:"method"`
	Params  json.RawMessage `json:"params,omitempty"`
	ID      *json.RawMessage `json:"id,omitempty"`
}

// Response represents a JSON-RPC 2.0 response
// https://www.jsonrpc.org/specification#response_object
type Response struct {
	JSONRPC string          `json:"jsonrpc"`
	Result  json.RawMessage `json:"result,omitempty"`
	Error   *Error          `json:"error,omitempty"`
	ID      *json.RawMessage `json:"id,omitempty"`
}

// Error represents a JSON-RPC 2.0 error
// https://www.jsonrpc.org/specification#error_object
type Error struct {
	Code    int             `json:"code"`
	Message string          `json:"message"`
	Data    json.RawMessage `json:"data,omitempty"`
}

// Notification represents a JSON-RPC 2.0 notification (server -> client)
// https://www.jsonrpc.org/specification#notification
type Notification struct {
	JSONRPC string          `json:"jsonrpc"`
	Method  string          `json:"method"`
	Params  json.RawMessage `json:"params,omitempty"`
}

// Standard error codes
const (
	ParseError          = -32700
	InvalidRequest      = -32600
	MethodNotFound      = -32601
	InvalidParams       = -32602
	InternalError       = -32603
	ServerErrorStart    = -32099
	ServerErrorEnd      = -32000
	ServerNotInitialized = -32002
	UnknownErrorCode    = -32001
)

// Standard error messages
var errorMessages = map[int]string{
	ParseError:          "Parse error",
	InvalidRequest:      "Invalid Request",
	MethodNotFound:      "Method not found",
	InvalidParams:       "Invalid params",
	InternalError:       "Internal error",
	ServerNotInitialized: "Server not initialized",
}

// Error implements error interface
func (e *Error) Error() string {
	return fmt.Sprintf("jsonrpc error %d: %s", e.Code, e.Message)
}

// NewError creates a new JSON-RPC error
func NewError(code int, message string) *Error {
	return &Error{
		Code:    code,
		Message: message,
	}
}

// NewErrorWithCode creates a new JSON-RPC error from a standard code
func NewErrorWithCode(code int) *Error {
	msg, ok := errorMessages[code]
	if !ok {
		msg = "Unknown error"
	}
	return NewError(code, msg)
}

// NewParseError creates a parse error
func NewParseError(details string) *Error {
	e := NewErrorWithCode(ParseError)
	if details != "" {
		data, _ := json.Marshal(details)
		e.Data = data
	}
	return e
}

// NewInvalidRequestError creates an invalid request error
func NewInvalidRequestError(details string) *Error {
	e := NewErrorWithCode(InvalidRequest)
	if details != "" {
		data, _ := json.Marshal(details)
		e.Data = data
	}
	return e
}

// NewMethodNotFoundError creates a method not found error
func NewMethodNotFoundError(method string) *Error {
	return NewError(MethodNotFound, fmt.Sprintf("Method not found: %s", method))
}

// NewInvalidParamsError creates an invalid params error
func NewInvalidParamsError(details string) *Error {
	e := NewErrorWithCode(InvalidParams)
	if details != "" {
		data, _ := json.Marshal(details)
		e.Data = data
	}
	return e
}

// NewInternalError creates an internal error
func NewInternalError(details string) *Error {
	e := NewErrorWithCode(InternalError)
	if details != "" {
		data, _ := json.Marshal(details)
		e.Data = data
	}
	return e
}

// StringID creates a JSON-RPC ID from a string
func StringID(s string) *json.RawMessage {
	raw := json.RawMessage(fmt.Sprintf(`"%s"`, s))
	return &raw
}

// IntID creates a JSON-RPC ID from an int
func IntID(i int) *json.RawMessage {
	raw := json.RawMessage(fmt.Sprintf(`%d`, i))
	return &raw
}

// IDToString converts ID to string
func IDToString(id *json.RawMessage) string {
	if id == nil {
		return ""
	}
	var s string
	if err := json.Unmarshal(*id, &s); err == nil {
		return s
	}
	var i int
	if err := json.Unmarshal(*id, &i); err == nil {
		return fmt.Sprintf("%d", i)
	}
	return string(*id)
}
