// Package handler provides NapCat management handlers with JSON-RPC interface
package handler

import (
	"bufio"
	"fmt"
	"io"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"sync"
	"time"

	"napcat-daemon/pkg/jsonrpc"
)

// NapCatHandler handles NapCat process management
type NapCatHandler struct {
	mu        sync.RWMutex
	process   *os.Process
	cmd       *exec.Cmd
	isRunning bool
	startTime time.Time
	logBuffer *LogBuffer
	onStatusChange func(status jsonrpc.NapCatStatus)
}

// LogBuffer holds recent log lines
type LogBuffer struct {
	mu      sync.RWMutex
	lines   []string
	maxSize int
}

// NewLogBuffer creates a new log buffer
func NewLogBuffer(maxSize int) *LogBuffer {
	return &LogBuffer{
		lines:   make([]string, 0, maxSize),
		maxSize: maxSize,
	}
}

// Add adds a line to the buffer
func (lb *LogBuffer) Add(line string) {
	lb.mu.Lock()
	defer lb.mu.Unlock()

	lb.lines = append(lb.lines, line)
	if len(lb.lines) > lb.maxSize {
		lb.lines = lb.lines[1:]
	}
}

// Get returns a copy of all lines
func (lb *LogBuffer) Get() []string {
	lb.mu.RLock()
	defer lb.mu.RUnlock()

	result := make([]string, len(lb.lines))
	copy(result, lb.lines)
	return result
}

// NewNapCatHandler creates a new NapCat handler
func NewNapCatHandler() *NapCatHandler {
	return &NapCatHandler{
		logBuffer: NewLogBuffer(1000),
	}
}

// SetStatusChangeCallback sets the callback for status changes
func (h *NapCatHandler) SetStatusChangeCallback(callback func(status jsonrpc.NapCatStatus)) {
	h.mu.Lock()
	defer h.mu.Unlock()
	h.onStatusChange = callback
}

// notifyStatusChange notifies the status change callback
func (h *NapCatHandler) notifyStatusChange() {
	h.mu.RLock()
	callback := h.onStatusChange
	h.mu.RUnlock()

	if callback != nil {
		callback(h.GetStatus())
	}
}

// Start starts the NapCat process
func (h *NapCatHandler) Start(workDir string) (*jsonrpc.NapCatStartResult, error) {
	h.mu.Lock()
	defer h.mu.Unlock()

	if h.isRunning {
		return nil, fmt.Errorf("NapCat is already running")
	}

	// Use default work directory if not provided
	if workDir == "" {
		workDir = "."
	}

	// Get executable path
	exePath, err := h.findNapCatExecutable(workDir)
	if err != nil {
		return nil, fmt.Errorf("failed to find NapCat executable: %w", err)
	}

	// Prepare command based on OS
	var cmd *exec.Cmd
	if runtime.GOOS == "windows" {
		cmd = exec.Command("cmd", "/c", exePath)
	} else {
		cmd = exec.Command("bash", "-c", exePath)
	}

	cmd.Dir = workDir
	cmd.Env = os.Environ()

	// Set up output pipes
	stdout, err := cmd.StdoutPipe()
	if err != nil {
		return nil, fmt.Errorf("failed to create stdout pipe: %w", err)
	}
	stderr, err := cmd.StderrPipe()
	if err != nil {
		return nil, fmt.Errorf("failed to create stderr pipe: %w", err)
	}

	// Start process
	if err := cmd.Start(); err != nil {
		return nil, fmt.Errorf("failed to start NapCat: %w", err)
	}

	h.cmd = cmd
	h.process = cmd.Process
	h.isRunning = true
	h.startTime = time.Now()

	// Start log collection
	go h.collectLogs(stdout, stderr)

	// Notify status change
	go h.notifyStatusChange()

	return &jsonrpc.NapCatStartResult{
		PID:    h.process.Pid,
		Status: "started",
	}, nil
}

// Stop stops the NapCat process
func (h *NapCatHandler) Stop() (*jsonrpc.NapCatStopResult, error) {
	h.mu.Lock()
	defer h.mu.Unlock()

	if !h.isRunning || h.process == nil {
		return nil, fmt.Errorf("NapCat is not running")
	}

	// Try graceful shutdown first
	if err := h.process.Signal(os.Interrupt); err != nil {
		// Force kill if graceful shutdown fails
		if err := h.process.Kill(); err != nil {
			return nil, fmt.Errorf("failed to stop NapCat: %w", err)
		}
	}

	h.isRunning = false
	h.process = nil
	h.cmd = nil

	// Notify status change
	go h.notifyStatusChange()

	return &jsonrpc.NapCatStopResult{
		Status: "stopped",
	}, nil
}

// Restart restarts the NapCat process
func (h *NapCatHandler) Restart(workDir string) (*jsonrpc.NapCatRestartResult, error) {
	// Stop first
	_, err := h.Stop()
	if err != nil {
		// If not running, continue to start
		if err.Error() != "NapCat is not running" {
			return nil, err
		}
	}

	// Wait a moment for clean shutdown
	time.Sleep(500 * time.Millisecond)

	// Start again
	startResult, err := h.Start(workDir)
	if err != nil {
		return nil, err
	}

	return &jsonrpc.NapCatRestartResult{
		PID:    startResult.PID,
		Status: "restarted",
	}, nil
}

// GetStatus returns the current NapCat status
func (h *NapCatHandler) GetStatus() jsonrpc.NapCatStatus {
	h.mu.RLock()
	defer h.mu.RUnlock()

	status := jsonrpc.NapCatStatus{
		Running: h.isRunning,
	}

	if h.isRunning && h.process != nil {
		status.PID = h.process.Pid
		status.Uptime = int64(time.Since(h.startTime).Seconds())
	}

	return status
}

// GetLogs returns the recent logs
func (h *NapCatHandler) GetLogs() []string {
	return h.logBuffer.Get()
}

// IsRunning returns whether NapCat is currently running
func (h *NapCatHandler) IsRunning() bool {
	h.mu.RLock()
	defer h.mu.RUnlock()
	return h.isRunning
}

// GetLogBuffer returns the log buffer for external access
func (h *NapCatHandler) GetLogBuffer() *LogBuffer {
	return h.logBuffer
}

func (h *NapCatHandler) findNapCatExecutable(workDir string) (string, error) {
	// Possible executable names
	names := []string{"napcat", "napcat.exe", "NapCat", "NapCat.exe", "napcat.sh", "napcat.bat"}

	// Check in work directory
	for _, name := range names {
		path := filepath.Join(workDir, name)
		if info, err := os.Stat(path); err == nil && !info.IsDir() {
			return path, nil
		}
	}

	// Check in PATH
	for _, name := range names {
		if path, err := exec.LookPath(name); err == nil {
			return path, nil
		}
	}

	return "", fmt.Errorf("NapCat executable not found")
}

func (h *NapCatHandler) collectLogs(stdout, stderr io.Reader) {
	var wg sync.WaitGroup
	wg.Add(2)

	readPipe := func(reader io.Reader) {
		defer wg.Done()
		scanner := bufio.NewScanner(reader)
		for scanner.Scan() {
			h.logBuffer.Add(scanner.Text())
		}
	}

	go readPipe(stdout)
	go readPipe(stderr)

	wg.Wait()

	// Process has exited
	h.mu.Lock()
	wasRunning := h.isRunning
	h.isRunning = false
	h.mu.Unlock()

	// Notify status change if was running
	if wasRunning {
		h.notifyStatusChange()
	}
}
