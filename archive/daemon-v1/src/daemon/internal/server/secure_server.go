// Package server provides secure JSON-RPC over WebSocket server
package server

import (
	"context"
	"crypto/tls"
	"encoding/json"
	"fmt"
	"log"
	"net"
	"net/http"
	"os"
	"runtime"
	"strings"
	"sync"
	"time"

	"github.com/gorilla/websocket"
	"napcat-daemon/internal/config"
	"napcat-daemon/internal/handler"
	"napcat-daemon/pkg/jsonrpc"
	"napcat-daemon/pkg/security"
)

// SecureServer provides a security-hardened JSON-RPC server
type SecureServer struct {
	config           *config.Config
	router           *jsonrpc.Router
	handler          *handler.NapCatHandler
	clients          map[string]*SecureClient
	clientsMu        sync.RWMutex
	server           *http.Server
	broadcastCh      chan any

	// Security components
	challengeStore   *security.ChallengeStore
	sessionManager   *security.SessionManager
	jwtSigner        *security.JWTSigner
	authLimiter      *security.AuthRateLimiter
	ipLimiter        *security.IPRateLimiter
	connLimiter      *security.ConnectionLimiter
	auditLogger      *security.AuditLogger
	validator        *security.Validator

	// TLS config
	tlsConfig        *tls.Config
	requireTLS       bool
}

// SecureClient represents a security-aware connected client
type SecureClient struct {
	id            string
	conn          *websocket.Conn
	server        *SecureServer
	send          chan []byte
	authenticated bool
	sessionID     string
	clientIP      string
	mu            sync.RWMutex
}

// IsAuthenticated returns whether the client is authenticated
func (c *SecureClient) IsAuthenticated() bool {
	c.mu.RLock()
	defer c.mu.RUnlock()
	return c.authenticated
}

// SetAuthenticated sets the authentication status
func (c *SecureClient) SetAuthenticated(auth bool) {
	c.mu.Lock()
	defer c.mu.Unlock()
	c.authenticated = auth
}

// NewSecure creates a new secure server instance
func NewSecure(cfg *config.Config, requireTLS bool) (*SecureServer, error) {
	s := &SecureServer{
		config:      cfg,
		router:      jsonrpc.NewRouter(),
		handler:     handler.NewNapCatHandler(),
		clients:     make(map[string]*SecureClient),
		broadcastCh: make(chan any, 100),
		requireTLS:  requireTLS,

		// Initialize security components
		challengeStore: security.NewChallengeStore(5 * time.Minute),
		sessionManager: security.NewSessionManager(1 * time.Hour),
		authLimiter:    security.NewAuthRateLimiter(),
		ipLimiter:      security.NewIPRateLimiter(10, 20), // 10/sec, burst 20
		connLimiter:    security.NewConnectionLimiter(5),  // 5 per IP
		validator:      security.NewValidator(nil),
	}

	// Initialize JWT signer with random secret if not configured
	jwtSecret := cfg.Auth.Token
	if jwtSecret == "" {
		jwtSecret, _ = security.GenerateSecureToken(32)
	}
	s.jwtSigner = security.NewJWTSigner(jwtSecret, "napcat-daemon", "napcat-desktop")

	// Initialize audit logger
	var err error
	s.auditLogger, err = security.NewAuditLogger("/var/log/napcat-audit.log", security.SeverityInfo)
	if err != nil {
		log.Printf("Warning: Failed to create audit logger: %v", err)
		// Continue without audit logging
	}

	// Register handlers
	s.registerSecureHandlers()

	// Setup HTTP handlers
	mux := http.NewServeMux()
	mux.HandleFunc("/ws", s.handleWebSocket)
	mux.HandleFunc("/health", s.handleHealth)
	mux.HandleFunc("/challenge", s.handleChallengeHTTP)

	s.server = &http.Server{
		Addr:         cfg.Server.Bind,
		Handler:      mux,
		ReadTimeout:  10 * time.Second,
		WriteTimeout: 10 * time.Second,
	}

	return s, nil
}

// SetTLSConfig configures TLS for the server
func (s *SecureServer) SetTLSConfig(certFile, keyFile string) error {
	if certFile == "" || keyFile == "" {
		return fmt.Errorf("certificate and key files required")
	}

	cert, err := tls.LoadX509KeyPair(certFile, keyFile)
	if err != nil {
		return fmt.Errorf("failed to load TLS certificates: %w", err)
	}

	s.tlsConfig = &tls.Config{
		Certificates: []tls.Certificate{cert},
		MinVersion:   tls.VersionTLS13,
		CipherSuites: []uint16{
			tls.TLS_AES_256_GCM_SHA384,
			tls.TLS_CHACHA20_POLY1305_SHA256,
			tls.TLS_AES_128_GCM_SHA256,
		},
	}

	s.server.TLSConfig = s.tlsConfig
	return nil
}

// registerSecureHandlers registers all secure JSON-RPC method handlers
func (s *SecureServer) registerSecureHandlers() {
	// Auth challenge
	s.router.RegisterFunc("auth.challenge", func(ctx context.Context, method string, params json.RawMessage) (any, *jsonrpc.Error) {
		return s.handleAuthChallenge(ctx)
	})

	// Auth authenticate (HMAC verification)
	s.router.RegisterFunc("auth.authenticate", func(ctx context.Context, method string, params json.RawMessage) (any, *jsonrpc.Error) {
		return s.handleAuthAuthenticate(ctx, params)
	})

	// Protected methods - require authentication
	protectedMethods := map[string]func(context.Context, string, json.RawMessage) (any, *jsonrpc.Error){
		jsonrpc.MethodNapCatStart:   s.handleNapCatStart,
		jsonrpc.MethodNapCatStop:    s.handleNapCatStop,
		jsonrpc.MethodNapCatRestart: s.handleNapCatRestart,
		jsonrpc.MethodNapCatStatus:  s.handleNapCatStatus,
		jsonrpc.MethodNapCatLogs:    s.handleNapCatLogs,
		jsonrpc.MethodConfigGet:     s.handleConfigGet,
		jsonrpc.MethodSystemInfo:    s.handleSystemInfo,
	}

	for method, handler := range protectedMethods {
		s.router.RegisterFunc(method, s.requireAuth(handler))
	}
}

// requireAuth wraps a handler to require authentication
func (s *SecureServer) requireAuth(handler func(context.Context, string, json.RawMessage) (any, *jsonrpc.Error)) func(context.Context, string, json.RawMessage) (any, *jsonrpc.Error) {
	return func(ctx context.Context, method string, params json.RawMessage) (any, *jsonrpc.Error) {
		// Get session from context
		session, ok := ctx.Value("session").(*security.Session)
		if !ok || session == nil {
			return nil, jsonrpc.NewError(-32001, "Unauthorized: authentication required")
		}

		// Check permissions
		if !s.hasPermission(session, method) {
			s.auditLogger.LogPermissionDenied(session.ClientIP, session.ID, method, "")
			return nil, jsonrpc.NewError(-32003, "Forbidden: insufficient permissions")
		}

		return handler(ctx, method, params)
	}
}

// hasPermission checks if a session has permission for a method
func (s *SecureServer) hasPermission(session *security.Session, method string) bool {
	// Admin has all permissions
	if session.HasPermission(security.PermAdmin) {
		return true
	}

	// Method-specific permission checks
	methodPerms := map[string][]string{
		jsonrpc.MethodNapCatStart:   {security.PermStatusWrite},
		jsonrpc.MethodNapCatStop:    {security.PermStatusWrite},
		jsonrpc.MethodNapCatRestart: {security.PermStatusWrite},
		jsonrpc.MethodNapCatStatus:  {security.PermStatusRead},
		jsonrpc.MethodNapCatLogs:    {security.PermStatusRead},
		jsonrpc.MethodConfigGet:     {security.PermConfigRead},
		jsonrpc.MethodConfigSet:     {security.PermConfigWrite},
	}

	requiredPerms, exists := methodPerms[method]
	if !exists {
		return true // No specific permissions required
	}

	for _, perm := range requiredPerms {
		if !session.HasPermission(perm) {
			return false
		}
	}
	return true
}

// Handler implementations
func (s *SecureServer) handleAuthChallenge(ctx context.Context) (any, *jsonrpc.Error) {
	clientIP, _ := ctx.Value("client_ip").(string)

	// Check rate limit
	if s.authLimiter.IsBlocked(clientIP) {
		s.auditLogger.LogRateLimit(clientIP, "auth_challenge")
		return nil, jsonrpc.NewError(-32002, "Rate limit exceeded")
	}

	// Generate challenge
	challenge, err := s.challengeStore.Generate()
	if err != nil {
		return nil, jsonrpc.NewInternalError(err.Error())
	}

	s.auditLogger.LogAuthChallenge(clientIP, true)

	return map[string]any{
		"challenge":  challenge.Value,
		"expires_at": challenge.ExpiresAt.Unix(),
		"algorithm":  "HMAC-SHA256",
	}, nil
}

func (s *SecureServer) handleAuthAuthenticate(ctx context.Context, params json.RawMessage) (any, *jsonrpc.Error) {
	clientIP, _ := ctx.Value("client_ip").(string)

	// Parse params
	var req struct {
		Challenge string `json:"challenge"`
		Signature string `json:"signature"`
	}
	if err := json.Unmarshal(params, &req); err != nil {
		s.auditLogger.LogAuth(clientIP, "", false, fmt.Errorf("invalid params"))
		return nil, jsonrpc.NewInvalidParamsError(err.Error())
	}

	// Check rate limit
	if !s.authLimiter.RecordAttempt(clientIP, false) {
		s.auditLogger.LogRateLimit(clientIP, "auth_failure")
		return nil, jsonrpc.NewError(-32002, "Too many failed attempts")
	}

	// Verify HMAC
	if !s.challengeStore.Verify(req.Challenge, req.Signature, s.config.Auth.Token) {
		s.auditLogger.LogAuth(clientIP, "", false, fmt.Errorf("invalid signature"))
		return nil, jsonrpc.NewError(-32001, "Invalid credentials")
	}

	// Create session
	session := s.sessionManager.Create("", clientIP, security.DefaultPermissions())

	// Generate JWT
	token, err := s.jwtSigner.GenerateAccessToken(session.ID, clientIP, session.Permissions)
	if err != nil {
		return nil, jsonrpc.NewInternalError(err.Error())
	}

	s.auditLogger.LogAuth(clientIP, session.ID, true, nil)

	return map[string]any{
		"token":        token,
		"expires_in":   int64(s.jwtSigner.accessTTL.Seconds()),
		"session_id":   session.ID,
		"permissions":  session.Permissions,
	}, nil
}

func (s *SecureServer) handleNapCatStart(ctx context.Context, method string, params json.RawMessage) (any, *jsonrpc.Error) {
	var p jsonrpc.NapCatStartParams
	if len(params) > 0 {
		if err := json.Unmarshal(params, &p); err != nil {
			return nil, jsonrpc.NewInvalidParamsError(err.Error())
		}
	}

	// Validate work directory
	if p.WorkDir != "" {
		if err := s.validator.ValidateWorkDir(p.WorkDir); err != nil {
			s.auditLogger.LogSecurityEvent(security.EventPathTraversal, "", map[string]any{
				"attempted_path": p.WorkDir,
			})
			return nil, jsonrpc.NewInvalidParamsError("Invalid work directory")
		}
	}

	result, err := s.handler.Start(p.WorkDir)
	if err != nil {
		return nil, jsonrpc.NewInternalError(err.Error())
	}

	return result, nil
}

func (s *SecureServer) handleNapCatStop(ctx context.Context, method string, params json.RawMessage) (any, *jsonrpc.Error) {
	result, err := s.handler.Stop()
	if err != nil {
		return nil, jsonrpc.NewInternalError(err.Error())
	}
	return result, nil
}

func (s *SecureServer) handleNapCatRestart(ctx context.Context, method string, params json.RawMessage) (any, *jsonrpc.Error) {
	var p jsonrpc.NapCatRestartParams
	if len(params) > 0 {
		if err := json.Unmarshal(params, &p); err != nil {
			return nil, jsonrpc.NewInvalidParamsError(err.Error())
		}
	}

	if p.WorkDir != "" {
		if err := s.validator.ValidateWorkDir(p.WorkDir); err != nil {
			return nil, jsonrpc.NewInvalidParamsError("Invalid work directory")
		}
	}

	result, err := s.handler.Restart(p.WorkDir)
	if err != nil {
		return nil, jsonrpc.NewInternalError(err.Error())
	}
	return result, nil
}

func (s *SecureServer) handleNapCatStatus(ctx context.Context, method string, params json.RawMessage) (any, *jsonrpc.Error) {
	status := s.handler.GetStatus()
	return jsonrpc.NapCatStatusResult{Status: status}, nil
}

func (s *SecureServer) handleNapCatLogs(ctx context.Context, method string, params json.RawMessage) (any, *jsonrpc.Error) {
	logs := s.handler.GetLogs()
	return jsonrpc.NapCatLogsResult{Logs: logs}, nil
}

func (s *SecureServer) handleConfigGet(ctx context.Context, method string, params json.RawMessage) (any, *jsonrpc.Error) {
	var p jsonrpc.ConfigGetParams
	if len(params) > 0 {
		if err := json.Unmarshal(params, &p); err != nil {
			return nil, jsonrpc.NewInvalidParamsError(err.Error())
		}
	}

	value := s.config.Get(p.Key)
	return jsonrpc.ConfigGetResult{Value: value}, nil
}

func (s *SecureServer) handleSystemInfo(ctx context.Context, method string, params json.RawMessage) (any, *jsonrpc.Error) {
	return jsonrpc.SystemInfoResult{
		Info: jsonrpc.SystemInfo{
			Version:   "1.0.0",
			GoVersion: runtime.Version(),
			OS:        runtime.GOOS,
			Arch:      runtime.GOARCH,
			PID:       os.Getpid(),
		},
	}, nil
}

// handleWebSocket handles WebSocket connections
func (s *SecureServer) handleWebSocket(w http.ResponseWriter, r *http.Request) {
	clientIP := security.ExtractIP(r.RemoteAddr)

	// Check TLS requirement
	if s.requireTLS && r.TLS == nil {
		s.auditLogger.LogSecurityEvent(security.EventCommandBlocked, clientIP, map[string]any{
			"reason": "TLS required but not used",
		})
		http.Error(w, "TLS required", http.StatusForbidden)
		return
	}

	// Check connection limit
	if !s.connLimiter.Acquire(clientIP) {
		s.auditLogger.LogSecurityEvent(security.EventConnLimitHit, clientIP, nil)
		http.Error(w, "Too many connections", http.StatusTooManyRequests)
		return
	}
	defer s.connLimiter.Release(clientIP)

	// Check rate limit
	if !s.ipLimiter.Allow(clientIP) {
		s.auditLogger.LogRateLimit(clientIP, "connection")
		http.Error(w, "Rate limit exceeded", http.StatusTooManyRequests)
		return
	}

	// Upgrade connection
	upgrader := websocket.Upgrader{
		CheckOrigin: func(r *http.Request) bool {
			// Implement proper origin checking
			return true
		},
		ReadBufferSize:  1024,
		WriteBufferSize: 1024,
	}

	conn, err := upgrader.Upgrade(w, r, nil)
	if err != nil {
		log.Printf("Failed to upgrade connection: %v", err)
		return
	}

	client := &SecureClient{
		id:       generateClientID(),
		conn:     conn,
		server:   s,
		send:     make(chan []byte, 256),
		clientIP: clientIP,
	}

	s.registerClient(client)

	// Start client goroutines
	go client.writePump()
	go client.readPump()
}

func (s *SecureServer) handleHealth(w http.ResponseWriter, r *http.Request) {
	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(http.StatusOK)
	w.Write([]byte(`{"jsonrpc":"2.0","result":{"status":"healthy"}}`))
}

func (s *SecureServer) handleChallengeHTTP(w http.ResponseWriter, r *http.Request) {
	// HTTP endpoint for getting challenge
	clientIP := security.ExtractIP(r.RemoteAddr)

	challenge, err := s.challengeStore.Generate()
	if err != nil {
		http.Error(w, err.Error(), http.StatusInternalServerError)
		return
	}

	response := map[string]any{
		"challenge":  challenge.Value,
		"expires_at": challenge.ExpiresAt.Unix(),
		"algorithm":  "HMAC-SHA256",
	}

	w.Header().Set("Content-Type", "application/json")
	json.NewEncoder(w).Encode(response)
}

func (s *SecureServer) registerClient(client *SecureClient) {
	s.clientsMu.Lock()
	defer s.clientsMu.Unlock()
	s.clients[client.id] = client
	log.Printf("Client connected: %s from %s", client.id, client.clientIP)
}

func (s *SecureServer) unregisterClient(client *SecureClient) {
	s.clientsMu.Lock()
	defer s.clientsMu.Unlock()
	if _, ok := s.clients[client.id]; ok {
		delete(s.clients, client.id)
		close(client.send)
		log.Printf("Client disconnected: %s", client.id)
	}
}

// Start starts the server
func (s *SecureServer) Start() error {
	// Start broadcast handler
	go s.broadcastLoop()

	// Start status monitor
	go s.monitorLoop()

	listener, err := net.Listen("tcp", s.server.Addr)
	if err != nil {
		return fmt.Errorf("failed to create listener: %w", err)
	}

	log.Printf("Secure JSON-RPC server starting on %s (TLS: %v)", s.server.Addr, s.requireTLS)

	go func() {
		if s.tlsConfig != nil {
			err = s.server.ServeTLS(listener, "", "")
		} else {
			err = s.server.Serve(listener)
		}
		if err != nil && err != http.ErrServerClosed {
			log.Printf("Server error: %v", err)
		}
	}()

	return nil
}

// Stop gracefully stops the server
func (s *SecureServer) Stop() error {
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()

	// Close all client connections
	s.clientsMu.Lock()
	for _, client := range s.clients {
		client.conn.Close()
	}
	s.clients = make(map[string]*SecureClient)
	s.clientsMu.Unlock()

	// Close audit logger
	if s.auditLogger != nil {
		s.auditLogger.Close()
	}

	return s.server.Shutdown(ctx)
}

// broadcastLoop handles broadcast messages
func (s *SecureServer) broadcastLoop() {
	for msg := range s.broadcastCh {
		data, err := json.Marshal(msg)
		if err != nil {
			log.Printf("Failed to marshal broadcast message: %v", err)
			continue
		}

		s.clientsMu.RLock()
		clients := make([]*SecureClient, 0, len(s.clients))
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
				s.unregisterClient(client)
				client.conn.Close()
			}
		}
	}
}

// monitorLoop monitors NapCat status and broadcasts updates
func (s *SecureServer) monitorLoop() {
	ticker := time.NewTicker(5 * time.Second)
	defer ticker.Stop()

	for range ticker.C {
		status := s.handler.GetStatus()
		s.broadcastCh <- jsonrpc.Notification{
			JSONRPC: "2.0",
			Method:  jsonrpc.NotificationStatusUpdate,
			Params:  status,
		}
	}
}

// Client methods
func (c *SecureClient) readPump() {
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

		c.handleMessage(message)
	}
}

func (c *SecureClient) handleMessage(message []byte) {
	// Parse the message
	var req jsonrpc.Request
	if err := json.Unmarshal(message, &req); err != nil {
		resp := c.server.router.Process(context.Background(), message)
		c.sendResponse(resp)
		return
	}

	// Create context with client info
	ctx := context.WithValue(context.Background(), "client_ip", c.clientIP)

	// Handle auth methods
	if req.Method == "auth.challenge" || req.Method == "auth.authenticate" {
		resp := c.server.router.Process(ctx, message)
		c.sendResponse(resp)
		return
	}

	// For protected methods, verify JWT
	if req.ID != nil {
		// Check for JWT in params or headers
		var token string
		var params map[string]any
		if err := json.Unmarshal(req.Params, &params); err == nil {
			if t, ok := params["token"].(string); ok {
				token = t
			}
		}

		if token == "" {
			c.sendError(req.ID, -32001, "Unauthorized: token required")
			return
		}

		// Verify JWT
		claims, err := c.server.jwtSigner.ParseAndVerify(token)
		if err != nil {
			c.sendError(req.ID, -32001, fmt.Sprintf("Unauthorized: %v", err))
			return
		}

		// Get session
		session, exists := c.server.sessionManager.Get(claims.SessionID)
		if !exists {
			c.sendError(req.ID, -32001, "Unauthorized: invalid session")
			return
		}

		// Bind session to context
		ctx = context.WithValue(ctx, "session", session)
		ctx = context.WithValue(ctx, "claims", claims)

		// Update session activity
		c.server.sessionManager.Touch(session.ID)
	}

	// Process the request
	resp := c.server.router.Process(ctx, message)
	if req.ID != nil {
		c.sendResponse(resp)
	}
}

func (c *SecureClient) sendResponse(resp *jsonrpc.Response) {
	data, err := json.Marshal(resp)
	if err != nil {
		log.Printf("Failed to marshal response: %v", err)
		return
	}

	select {
	case c.send <- data:
	default:
		c.server.unregisterClient(c)
		c.conn.Close()
	}
}

func (c *SecureClient) sendError(id *json.RawMessage, code int, message string) {
	resp := &jsonrpc.Response{
		JSONRPC: "2.0",
		ID:      id,
		Error:   jsonrpc.NewError(code, message),
	}
	c.sendResponse(resp)
}

func (c *SecureClient) writePump() {
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

func generateClientID() string {
	return fmt.Sprintf("client_%d", time.Now().UnixNano())
}
