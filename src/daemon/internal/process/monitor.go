// Package process provides process monitoring capabilities
package process

import (
	"context"
	"fmt"
	"os"
	"runtime"
	"sync"
	"time"
)

// ProcessInfo holds information about a monitored process
type ProcessInfo struct {
	PID        int
	Name       string
	StartTime  time.Time
	CPUUsage   float64
	MemoryMB   float64
	Status     string
	IsRunning  bool
}

// Monitor handles process monitoring
type Monitor struct {
	mu        sync.RWMutex
	processes map[int]*ProcessInfo
	interval  time.Duration
	stopChan  chan struct{}
	wg        sync.WaitGroup
}

// NewMonitor creates a new process monitor
func NewMonitor(interval time.Duration) *Monitor {
	if interval <= 0 {
		interval = 5 * time.Second
	}
	return &Monitor{
		processes: make(map[int]*ProcessInfo),
		interval:  interval,
		stopChan:  make(chan struct{}),
	}
}

// Start begins the monitoring loop
func (m *Monitor) Start(ctx context.Context) {
	m.wg.Add(1)
	go m.monitoringLoop(ctx)
}

// Stop halts the monitoring loop
func (m *Monitor) Stop() {
	close(m.stopChan)
	m.wg.Wait()
}

// AddProcess adds a process to monitor
func (m *Monitor) AddProcess(pid int, name string) error {
	m.mu.Lock()
	defer m.mu.Unlock()

	// Check if process exists
	process, err := os.FindProcess(pid)
	if err != nil {
		return fmt.Errorf("process not found: %d", pid)
	}

	// Verify process is running (platform-specific)
	if !isProcessRunning(process) {
		return fmt.Errorf("process is not running: %d", pid)
	}

	m.processes[pid] = &ProcessInfo{
		PID:       pid,
		Name:      name,
		StartTime: time.Now(),
		Status:    "running",
		IsRunning: true,
	}

	return nil
}

// RemoveProcess removes a process from monitoring
func (m *Monitor) RemoveProcess(pid int) {
	m.mu.Lock()
	defer m.mu.Unlock()
	delete(m.processes, pid)
}

// GetProcessInfo returns information about a specific process
func (m *Monitor) GetProcessInfo(pid int) (*ProcessInfo, bool) {
	m.mu.RLock()
	defer m.mu.RUnlock()
	info, ok := m.processes[pid]
	return info, ok
}

// GetAllProcesses returns information about all monitored processes
func (m *Monitor) GetAllProcesses() []*ProcessInfo {
	m.mu.RLock()
	defer m.mu.RUnlock()

	result := make([]*ProcessInfo, 0, len(m.processes))
	for _, info := range m.processes {
		result = append(result, info)
	}
	return result
}

// monitoringLoop is the main monitoring goroutine
func (m *Monitor) monitoringLoop(ctx context.Context) {
	defer m.wg.Done()

	ticker := time.NewTicker(m.interval)
	defer ticker.Stop()

	for {
		select {
		case <-ctx.Done():
			return
		case <-m.stopChan:
			return
		case <-ticker.C:
			m.updateProcessStats()
		}
	}
}

// updateProcessStats updates statistics for all monitored processes
func (m *Monitor) updateProcessStats() {
	m.mu.Lock()
	defer m.mu.Unlock()

	for pid, info := range m.processes {
		process, err := os.FindProcess(pid)
		if err != nil {
			info.IsRunning = false
			info.Status = "not_found"
			continue
		}

		if !isProcessRunning(process) {
			info.IsRunning = false
			info.Status = "stopped"
			continue
		}

		// Update platform-specific stats
		m.updatePlatformStats(info)
	}
}

// isProcessRunning checks if a process is running (platform-specific)
func isProcessRunning(process *os.Process) bool {
	// On Unix systems, Signal(0) checks if process exists
	// On Windows, this needs a different approach
	err := process.Signal(os.Signal(nil))
	return err == nil
}

// updatePlatformStats updates platform-specific statistics
func (m *Monitor) updatePlatformStats(info *ProcessInfo) {
	// Platform-specific implementation would go here
	// For now, just update the timestamp
	info.IsRunning = true
	info.Status = "running"

	// CPU and memory stats require platform-specific code
	// Using runtime stats as a rough approximation for the daemon itself
	if info.PID == os.Getpid() {
		var mStats runtime.MemStats
		runtime.ReadMemStats(&mStats)
		info.MemoryMB = float64(mStats.Alloc) / 1024 / 1024
	}
}

// GetSystemStats returns overall system statistics
func (m *Monitor) GetSystemStats() map[string]interface{} {
	stats := map[string]interface{}{
		"go_version":   runtime.Version(),
		"os":           runtime.GOOS,
		"arch":         runtime.GOARCH,
		"num_cpu":      runtime.NumCPU(),
		"goroutines":   runtime.NumGoroutine(),
	}

	var mStats runtime.MemStats
	runtime.ReadMemStats(&mStats)
	stats["memory_alloc_mb"] = float64(mStats.Alloc) / 1024 / 1024
	stats["memory_sys_mb"] = float64(mStats.Sys) / 1024 / 1024

	return stats
}

// WatchProcess monitors a specific process and calls callback on state changes
func (m *Monitor) WatchProcess(pid int, callback func(*ProcessInfo)) {
	go func() {
		ticker := time.NewTicker(m.interval)
		defer ticker.Stop()

		var lastStatus string

		for {
			select {
			case <-m.stopChan:
				return
			case <-ticker.C:
				info, ok := m.GetProcessInfo(pid)
				if !ok {
					return
				}

				if info.Status != lastStatus {
					lastStatus = info.Status
					callback(info)
				}
			}
		}
	}()
}
