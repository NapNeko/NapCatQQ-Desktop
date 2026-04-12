// Package server provides JSON-RPC over WebSocket server
package server

import (
	"context"
	"encoding/json"
	"fmt"
	"log"
	"net"
	"net/http"
	"os"
	"runtime"
	"sync"
	"time"

	"github.com/gorilla/websocket"
	"napcat-daemon/internal/config"
	"napcat-daemon/internal/handler"
	"napcat-daemon/pkg/jsonrpc"
)

var upgrader = websocket.Upgrader{
	CheckOrigin: func(r *http.Request) bool {
		// In production, implement proper origin checking
		return true
	},
	ReadBufferSize:  1024,
	WriteBufferSize: 1024,
}

// Server represents the JSON-RPC WebSocket server
type Server struct {
	config      *config.Config
	router      *jsonrpc.Router
	clients     map[string]*Client
	clientsMu   sync.RWMutex
	server      *http.Server
	broadcastCh chan any
}

// Client represents a connected client
type Client struct {
	id            string
	conn          *websocket.Conn
	server        *Server
	send          chan []byte
	authenticated bool
	mu            sync.RWMutex
}

// IsAuthenticated returns whether the client is authenticated
func (c *Client) IsAuthenticated() bool {
	c.mu.RLock()
	defer c.mu.RUnlock()
	return c.authenticated
}

// SetAuthenticated sets the authentication status
func (c *Client) SetAuthenticated(auth bool) {
	c.mu.Lock()
	defer c.mu.Unlock()
	c.authenticated = auth
}

// New creates a new server instance
func New(cfg *config.Config) *Server {
	s := &Server{
		config:      cfg,
		router:      jsonrpc.NewRouter(),
		clients:     make(map[string]*Client),
		broadcastCh: make(chan any, 100),
	}

	// Register handlers
	s.registerHandlers()

	mux := http.NewServeMux()
	mux.HandleFunc("/ws", s.handleWebSocket)
	mux.HandleFunc("/health", s.handleHealth)

	s.server = &http.Server{
		Addr:    cfg.Server.Bind,
		Handler: mux,
	}

	return s
}

// registerHandlers registers all JSON-RPC method handlers
func (s *Server) registerHandlers() {
	napcatHandler := handler.NewNapCatHandler()

	// Auth
	s.router.RegisterFunc(jsonrpc.MethodAuthAuthenticate, func(ctx context.Context, method string, params json.RawMessage) (any, *jsonrpc.Error) {
		var p jsonrpc.AuthAuthenticateParams
		if err := json.Unmarshal(params, &p); err != nil {
			return nil, jsonrpc.NewInvalidParamsError(err.Error())
		}

		if p.Token != s.config.Auth.Token {
			return jsonrpc.AuthAuthenticateResult{Authenticated: false}, nil
		}

		// Mark client as authenticated (need to get client from context)
		return jsonrpc.AuthAuthenticateResult{Authenticated: true}, nil
	})

	// NapCat control
	s.router.RegisterFunc(jsonrpc.MethodNapCatStart, func(ctx context.Context, method string, params json.RawMessage) (any, *jsonrpc.Error) {
		var p jsonrpc.NapCatStartParams
		if len(params) > 0 {
			if err := json.Unmarshal(params, &p); err != nil {
				return nil, jsonrpc.NewInvalidParamsError(err.Error())
			}
		}

		result, rpcErr := napcatHandler.Start(p.WorkDir)
		if rpcErr != nil {
			return nil, jsonrpc.NewInternalError(rpcErr.Error())
		}
		return result, nil
	})

	s.router.RegisterFunc(jsonrpc.MethodNapCatStop, func(ctx context.Context, method string, params json.RawMessage) (any, *jsonrpc.Error) {
		result, rpcErr := napcatHandler.Stop()
		if rpcErr != nil {
			return nil, jsonrpc.NewInternalError(rpcErr.Error())
		}
		return result, nil
	})

	s.router.RegisterFunc(jsonrpc.MethodNapCatRestart, func(ctx context.Context, method string, params json.RawMessage) (any, *jsonrpc.Error) {
		var p jsonrpc.NapCatRestartParams
		if len(params) > 0 {
			if err := json.Unmarshal(params, &p); err != nil {
				return nil, jsonrpc.NewInvalidParamsError(err.Error())
			}
		}

		result, rpcErr := napcatHandler.Restart(p.WorkDir)
		if rpcErr != nil {
			return nil, jsonrpc.NewInternalError(rpcErr.Error())
		}
		return result, nil
	})

	s.router.RegisterFunc(jsonrpc.MethodNapCatStatus, func(ctx context.Context, method string, params json.RawMessage) (any, *jsonrpc.Error) {
		status := napcatHandler.GetStatus()
		return jsonrpc.NapCatStatusResult{Status: status}, nil
	})

	s.router.RegisterFunc(jsonrpc.MethodNapCatLogs, func(ctx context.Context, method string, params json.RawMessage) (any, *jsonrpc.Error) {
		logs := napcatHandler.GetLogs()
		return jsonrpc.NapCatLogsResult{Logs: logs}, nil
	})

	// Config
	s.router.RegisterFunc(jsonrpc.MethodConfigGet, func(ctx context.Context, method string, params json.RawMessage) (any, *jsonrpc.Error) {
		var p jsonrpc.ConfigGetParams
		if len(params) > 0 {
			if err := json.Unmarshal(params, &p); err != nil {
				return nil, jsonrpc.NewInvalidParamsError(err.Error())
			}
		}

		value := s.config.Get(p.Key)
		return jsonrpc.ConfigGetResult{Value: value}, nil
	})

	// System
	s.router.RegisterFunc(jsonrpc.MethodSystemInfo, func(ctx context.Context, method string, params json.RawMessage) (any, *jsonrpc.Error) {
		return jsonrpc.SystemInfoResult{
			Info: jsonrpc.SystemInfo{
				Version:   "1.0.0",
				GoVersion: runtime.Version(),
				OS:        runtime.GOOS,
				Arch:      runtime.GOARCH,
				PID:       os.Getpid(),
			},
		}, nil
	})
}

// Start starts the server
func (s *Server) Start() error {
	// Start broadcast handler
	go s.broadcastLoop()

	// Start status monitor
	go s.monitorLoop()

	listener, err := net.Listen("tcp", s.server.Addr)
	if err != nil {
		return fmt.Errorf("failed to create listener: %w", err)
	}

	log.Printf("JSON-RPC server starting on %s", s.server.Addr)

	go func() {
		if err := s.server.Serve(listener); err != nil && err != http.ErrServerClosed {
			log.Printf("Server error: %v", err)
		}
	}()

	return nil
}

// Stop gracefully stops the server
func (s *Server) Stop() error {
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()

	// Close all client connections
	s.clientsMu.Lock()
	for _, client := range s.clients {
		client.conn.Close()
	}
	s.clients = make(map[string]*Client)
	s.clientsMu.Unlock()

	return s.server.Shutdown(ctx)
}

// handleWebSocket handles WebSocket connections
func (s *Server) handleWebSocket(w http.ResponseWriter, r *http.Request) {
	conn, err := upgrader.Upgrade(w, r, nil)
	if err != nil {
		log.Printf("Failed to upgrade connection: %v", err)
		return
	}

	client := &Client{
		id:     generateClientID(),
		conn:   conn,
		server: s,
		send:   make(chan []byte, 256),
	}

	s.registerClient(client)

	// Start client goroutines
	go client.writePump()
	go client.readPump()
}

// handleHealth handles health check requests
func (s *Server) handleHealth(w http.ResponseWriter, r *http.Request) {
	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(http.StatusOK)
	w.Write([]byte(`{"jsonrpc":"2.0","result":{"status":"healthy"}}`))
}

// registerClient registers a new client
func (s *Server) registerClient(client *Client) {
	s.clientsMu.Lock()
	defer s.clientsMu.Unlock()
	s.clients[client.id] = client
	log.Printf("Client connected: %s", client.id)
}

// unregisterClient unregisters a client
func (s *Server) unregisterClient(client *Client) {
	s.clientsMu.Lock()
	defer s.clientsMu.Unlock()
	if _, ok := s.clients[client.id]; ok {
		delete(s.clients, client.id)
		close(client.send)
		log.Printf("Client disconnected: %s", client.id)
	}
}

// broadcastLoop handles broadcast messages
func (s *Server) broadcastLoop() {
	for msg := range s.broadcastCh {
		data, err := json.Marshal(msg)
		if err != nil {
			log.Printf("Failed to marshal broadcast message: %v", err)
			continue
		}

		s.clientsMu.RLock()
		clients := make([]*Client, 0, len(s.clients))
		for _, client := range s.clients {
			if client.IsAuthenticated() {
				clients = append(clients, client)
			}
		}
		s.clientsMu.RUnlock()

		for _, client := range clients {
			select {
			case client.send <- data:
			default:
				// Client send buffer full, close connection
				s.unregisterClient(client)
				client.conn.Close()
			}
		}
	}
}

// monitorLoop monitors NapCat status and broadcasts updates
func (s *Server) monitorLoop() {
	ticker := time.NewTicker(5 * time.Second)
	defer ticker.Stop()

	for range ticker.C {
		// This would get status from the handler and broadcast
		// Implementation depends on how you want to handle state
	}
}

// Broadcast sends a notification to all authenticated clients
func (s *Server) Broadcast(method string, params any) {
	notification := jsonrpc.Notification{
		JSONRPC: "2.0",
		Method:  method,
	}

	paramsData, err := json.Marshal(params)
	if err != nil {
		log.Printf("Failed to marshal notification params: %v", err)
		return
	}
	notification.Params = paramsData

	s.broadcastCh <- notification
}

// readPump reads messages from the client
func (c *Client) readPump() {
	defer func() {
		c.server.unregisterClient(c)
		c.conn.Close()
	}()

	c.conn.SetReadDeadline(time.Now().Add(60 * time.Second))
	c.conn.SetPongHandler(func(string) error {
		c.conn.SetReadDeadline(time.Now().Add(60 * time.Second))
		return nil
	})

	for {
		_, message, err := c.conn.ReadMessage()
		if err != nil {
			if websocket.IsUnexpectedCloseError(err, websocket.CloseGoingAway, websocket.CloseAbnormalClosure) {
				log.Printf("WebSocket error: %v", err)
			}
			break
		}

		// Process the message
		c.handleMessage(message)
	}
}

// handleMessage processes a single message
func (c *Client) handleMessage(message []byte) {
	// Try to parse as request to get method
	var req struct {
		JSONRPC string          `json:"jsonrpc"`
		Method  string          `json:"method"`
		ID      json.RawMessage `json:"id"`
	}

	if err := json.Unmarshal(message, &req); err != nil {
		// Send parse error
		resp := c.server.router.Process(context.Background(), message)
		c.sendResponse(resp)
		return
	}

	// Check if it's a notification (no ID)
	isNotification := len(req.ID) == 0 || string(req.ID) == "null"

	// Special handling for auth - must be first call
	if req.Method == jsonrpc.MethodAuthAuthenticate {
		resp := c.server.router.Process(context.Background(), message)
		if resp.Error == nil && resp.Result != nil {
			var result jsonrpc.AuthAuthenticateResult
			if err := json.Unmarshal(resp.Result, &result); err == nil && result.Authenticated {
				c.SetAuthenticated(true)
			}
		}
		if !isNotification {
			c.sendResponse(resp)
		}
		return
	}

	// Require authentication for other methods
	if !c.IsAuthenticated() {
		if !isNotification {
			resp := &jsonrpc.Response{
				JSONRPC: "2.0",
				ID:      req.ID,
				Error:   jsonrpc.NewError(-32001, "Unauthorized: authentication required"),
			}
			c.sendResponse(resp)
		}
		return
	}

	// Process the request
	resp := c.server.router.Process(context.Background(), message)
	if !isNotification {
		c.sendResponse(resp)
	}
}

// sendResponse sends a response to the client
func (c *Client) sendResponse(resp *jsonrpc.Response) {
	data, err := json.Marshal(resp)
	if err != nil {
		log.Printf("Failed to marshal response: %v", err)
		return
	}

	select {
	case c.send <- data:
	default:
		// Buffer full, close connection
		c.server.unregisterClient(c)
		c.conn.Close()
	}
}

// writePump writes messages to the client
func (c *Client) writePump() {
	ticker := time.NewTicker(30 * time.Second)
	defer func() {
		ticker.Stop()
		c.conn.Close()
	}()

	for {
		select {
		case message, ok := <-c.send:
			c.conn.SetWriteDeadline(time.Now().Add(10 * time.Second))
			if !ok {
				c.conn.WriteMessage(websocket.CloseMessage, []byte{})
				return
			}
			if err := c.conn.WriteMessage(websocket.TextMessage, message); err != nil {
				log.Printf("Failed to write message: %v", err)
				return
			}

		case <-ticker.C:
			c.conn.SetWriteDeadline(time.Now().Add(10 * time.Second))
			if err := c.conn.WriteMessage(websocket.PingMessage, nil); err != nil {
				return
			}
		}
	}
}

// generateClientID generates a unique client ID
func generateClientID() string {
	return fmt.Sprintf("client_%d", time.Now().UnixNano())
}
