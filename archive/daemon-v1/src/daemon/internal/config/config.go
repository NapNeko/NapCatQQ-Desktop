// Package config provides configuration management for the daemon
package config

import (
	"crypto/rand"
	"encoding/hex"
	"fmt"
	"os"
	"path/filepath"

	"gopkg.in/yaml.v3"
)

// Config represents the daemon configuration
type Config struct {
	Server ServerConfig `yaml:"server"`
	Auth   AuthConfig   `yaml:"auth"`
	NapCat NapCatConfig `yaml:"napcat"`
	Log    LogConfig    `yaml:"logging"`
}

// ServerConfig contains server-related settings
type ServerConfig struct {
	Bind string    `yaml:"bind"`
	TLS  TLSConfig `yaml:"tls"`
}

// TLSConfig contains TLS settings
type TLSConfig struct {
	Cert string `yaml:"cert"`
	Key  string `yaml:"key"`
}

// AuthConfig contains authentication settings
type AuthConfig struct {
	Token string `yaml:"token"`
}

// NapCatConfig contains NapCat-specific settings
type NapCatConfig struct {
	Workspace  string `yaml:"workspace"`
	AutoStart  bool   `yaml:"auto_start"`
}

// LogConfig contains logging settings
type LogConfig struct {
	Level string `yaml:"level"`
	File  string `yaml:"file"`
}

// DefaultConfig returns the default configuration
func DefaultConfig() *Config {
	return &Config{
		Server: ServerConfig{
			Bind: "0.0.0.0:8443",
			TLS: TLSConfig{
				Cert: "/etc/napcat-daemon/server.crt",
				Key:  "/etc/napcat-daemon/server.key",
			},
		},
		Auth: AuthConfig{
			Token: generateToken(),
		},
		NapCat: NapCatConfig{
			Workspace: "$HOME/Napcat",
			AutoStart: false,
		},
		Log: LogConfig{
			Level: "info",
			File:  "/var/log/napcat-daemon.log",
		},
	}
}

// Load reads configuration from file
func Load(path string) (*Config, error) {
	data, err := os.ReadFile(path)
	if err != nil {
		if os.IsNotExist(err) {
			return DefaultConfig(), nil
		}
		return nil, fmt.Errorf("failed to read config file: %w", err)
	}

	var cfg Config
	if err := yaml.Unmarshal(data, &cfg); err != nil {
		return nil, fmt.Errorf("failed to parse config: %w", err)
	}

	// Expand environment variables
	cfg.NapCat.Workspace = os.ExpandEnv(cfg.NapCat.Workspace)

	return &cfg, nil
}

// Save writes configuration to file
func (c *Config) Save(path string) error {
	data, err := yaml.Marshal(c)
	if err != nil {
		return fmt.Errorf("failed to marshal config: %w", err)
	}

	// Ensure directory exists
	dir := filepath.Dir(path)
	if err := os.MkdirAll(dir, 0755); err != nil {
		return fmt.Errorf("failed to create config directory: %w", err)
	}

	if err := os.WriteFile(path, data, 0600); err != nil {
		return fmt.Errorf("failed to write config file: %w", err)
	}

	return nil
}

// Get returns a configuration value by key
// If key is empty, returns all configuration values
func (c *Config) Get(key string) map[string]any {
	result := make(map[string]any)

	if key == "" {
		// Return all config
		result["server"] = map[string]any{
			"bind": c.Server.Bind,
			"tls": map[string]any{
				"enabled": c.Server.TLS.Cert != "" && c.Server.TLS.Key != "",
			},
		}
		result["napcat"] = map[string]any{
			"workspace":  c.NapCat.Workspace,
			"auto_start": c.NapCat.AutoStart,
		}
		result["logging"] = map[string]any{
			"level": c.Log.Level,
			"file":  c.Log.File,
		}
		return result
	}

	// Return specific key
	switch key {
	case "server.bind":
		result["value"] = c.Server.Bind
	case "napcat.workspace":
		result["value"] = c.NapCat.Workspace
	case "napcat.auto_start":
		result["value"] = c.NapCat.AutoStart
	case "logging.level":
		result["value"] = c.Log.Level
	}

	return result
}

// RunSetup performs initial setup
func RunSetup() error {
	cfg := DefaultConfig()

	// Create config directory
	configDir := "/etc/napcat-daemon"
	if err := os.MkdirAll(configDir, 0755); err != nil {
		return fmt.Errorf("failed to create config directory: %w", err)
	}

	// Save configuration
	configPath := filepath.Join(configDir, "config.yaml")
	if err := cfg.Save(configPath); err != nil {
		return err
	}

	fmt.Printf("Configuration saved to: %s\n", configPath)
	fmt.Printf("Token: %s\n", cfg.Auth.Token)
	fmt.Println("\nPlease save this token securely. It will not be shown again.")

	return nil
}

// generateToken generates a random authentication token
func generateToken() string {
	bytes := make([]byte, 32)
	if _, err := rand.Read(bytes); err != nil {
		// Fallback to a default token (should not happen in practice)
		return "napcat-default-token-change-immediately"
	}
	return hex.EncodeToString(bytes)
}
