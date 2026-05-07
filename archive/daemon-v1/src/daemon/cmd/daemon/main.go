// NapCat Daemon - Remote management service for NapCatQQ
package main

import (
	"flag"
	"log"
	"os"
	"os/signal"
	"syscall"

	"napcat-daemon/internal/config"
	"napcat-daemon/internal/server"
)

var (
	configPath = flag.String("config", "/etc/napcat-daemon/config.yaml", "Path to config file")
	setupMode  = flag.Bool("setup", false, "Run initial setup")
)

func main() {
	flag.Parse()

	if *setupMode {
		runSetup()
		return
	}

	// Load configuration
	cfg, err := config.Load(*configPath)
	if err != nil {
		log.Fatalf("Failed to load config: %v", err)
	}

	// Create and start server
	srv := server.New(cfg)
	if err := srv.Start(); err != nil {
		log.Fatalf("Failed to start server: %v", err)
	}

	// Wait for interrupt signal
	sigChan := make(chan os.Signal, 1)
	signal.Notify(sigChan, syscall.SIGINT, syscall.SIGTERM)
	<-sigChan

	log.Println("Shutting down...")
	if err := srv.Stop(); err != nil {
		log.Printf("Error during shutdown: %v", err)
	}
}

func runSetup() {
	log.Println("Running initial setup...")
	if err := config.RunSetup(); err != nil {
		log.Fatalf("Setup failed: %v", err)
	}
	log.Println("Setup completed successfully")
}
