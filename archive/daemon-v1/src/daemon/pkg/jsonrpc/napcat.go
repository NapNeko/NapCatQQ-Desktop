// Package jsonrpc provides NapCat-specific JSON-RPC types
package jsonrpc

// NapCatStartParams represents parameters for napcat.start
// https://www.jsonrpc.org/specification#parameter_structures
type NapCatStartParams struct {
	WorkDir string `json:"work_dir,omitempty"`
}

// NapCatStartResult represents the result of napcat.start
type NapCatStartResult struct {
	PID    int    `json:"pid"`
	Status string `json:"status"`
}

// NapCatStopResult represents the result of napcat.stop
type NapCatStopResult struct {
	Status string `json:"status"`
}

// NapCatRestartParams represents parameters for napcat.restart
type NapCatRestartParams struct {
	WorkDir string `json:"work_dir,omitempty"`
}

// NapCatRestartResult represents the result of napcat.restart
type NapCatRestartResult struct {
	PID    int    `json:"pid"`
	Status string `json:"status"`
}

// NapCatStatus represents NapCat runtime status
type NapCatStatus struct {
	Running  bool   `json:"running"`
	PID      int    `json:"pid,omitempty"`
	QQ       string `json:"qq,omitempty"`
	Version  string `json:"version,omitempty"`
	LogFile  string `json:"log_file,omitempty"`
	Uptime   int64  `json:"uptime,omitempty"`
}

// NapCatStatusResult represents the result of napcat.status
type NapCatStatusResult struct {
	Status NapCatStatus `json:"status"`
}

// NapCatLogsResult represents the result of napcat.logs
type NapCatLogsResult struct {
	Logs []string `json:"logs"`
}

// ConfigGetParams represents parameters for config.get
type ConfigGetParams struct {
	Key string `json:"key,omitempty"`
}

// ConfigGetResult represents the result of config.get
type ConfigGetResult struct {
	Value map[string]any `json:"value"`
}

// ConfigSetParams represents parameters for config.set
type ConfigSetParams struct {
	Key   string `json:"key"`
	Value any    `json:"value"`
}

// ConfigSetResult represents the result of config.set
type ConfigSetResult struct {
	Success bool `json:"success"`
}

// LogSubscribeParams represents parameters for log.subscribe
type LogSubscribeParams struct {
	Level   string `json:"level,omitempty"`
	Pattern string `json:"pattern,omitempty"`
}

// LogSubscribeResult represents the result of log.subscribe
type LogSubscribeResult struct {
	Subscribed bool `json:"subscribed"`
}

// LogUnsubscribeResult represents the result of log.unsubscribe
type LogUnsubscribeResult struct {
	Unsubscribed bool `json:"unsubscribed"`
}

// FileUploadParams represents parameters for file.upload
type FileUploadParams struct {
	Path     string `json:"path"`
	Content  string `json:"content"`
	Encoding string `json:"encoding,omitempty"` // "hex", "base64", "utf8"
}

// FileUploadResult represents the result of file.upload
type FileUploadResult struct {
	Success bool `json:"success"`
}

// FileDownloadParams represents parameters for file.download
type FileDownloadParams struct {
	Path string `json:"path"`
}

// FileDownloadResult represents the result of file.download
type FileDownloadResult struct {
	Content  string `json:"content"`
	Encoding string `json:"encoding"` // "hex", "base64", "utf8"
}

// AuthAuthenticateParams represents parameters for auth.authenticate
type AuthAuthenticateParams struct {
	Token string `json:"token"`
}

// AuthAuthenticateResult represents the result of auth.authenticate
type AuthAuthenticateResult struct {
	Authenticated bool `json:"authenticated"`
}

// SystemInfo represents system information
type SystemInfo struct {
	Version   string `json:"version"`
	GoVersion string `json:"go_version"`
	OS        string `json:"os"`
	Arch      string `json:"arch"`
	PID       int    `json:"pid"`
}

// SystemInfoResult represents the result of system.info
type SystemInfoResult struct {
	Info SystemInfo `json:"info"`
}

// Standard method names
const (
	// Authentication
	MethodAuthAuthenticate = "auth.authenticate"

	// NapCat control
	MethodNapCatStart   = "napcat.start"
	MethodNapCatStop    = "napcat.stop"
	MethodNapCatRestart = "napcat.restart"
	MethodNapCatStatus  = "napcat.status"
	MethodNapCatLogs    = "napcat.logs"

	// Configuration
	MethodConfigGet = "config.get"
	MethodConfigSet = "config.set"

	// Log streaming
	MethodLogSubscribe   = "log.subscribe"
	MethodLogUnsubscribe = "log.unsubscribe"

	// File operations
	MethodFileUpload   = "file.upload"
	MethodFileDownload = "file.download"

	// System
	MethodSystemInfo = "system.info"
)

// Standard notification names
const (
	// Status updates
	NotificationStatusUpdate = "status.update"

	// Log entries
	NotificationLogEntry = "log.entry"

	// Process events
	NotificationProcessExit   = "process.exit"
	NotificationProcessStart  = "process.start"
	NotificationProcessError  = "process.error"
)

// StatusUpdateNotification represents a status update notification
type StatusUpdateNotification struct {
	Status NapCatStatus `json:"status"`
}

// LogEntryNotification represents a log entry notification
type LogEntryNotification struct {
	Timestamp int64  `json:"timestamp"`
	Level     string `json:"level"`
	Message   string `json:"message"`
	Source    string `json:"source,omitempty"`
}

// ProcessEventNotification represents a process event notification
type ProcessEventNotification struct {
	Event   string `json:"event"`
	PID     int    `json:"pid,omitempty"`
	Message string `json:"message,omitempty"`
}
