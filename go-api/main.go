package main

import (
	"context"
	"encoding/json"
	"fmt"
	"log"
	"net/http"
	"time"

	"github.com/ThreeDotsLabs/watermill"
	"github.com/ThreeDotsLabs/watermill-amqp/v2/pkg/amqp"
	"github.com/ThreeDotsLabs/watermill/message"
	"github.com/redis/go-redis/v9"
)

// --- Configuration Constants ---
const (
	amqpURI   = "amqp://guest:guest@localhost:5672/"
	redisAddr = "localhost:6379"
	topicName = "transactions"
)

// --- Data Models ---
type TransferRequest struct {
	IdempotencyKey string  `json:"idempotency_key"`
	SourceID       int     `json:"source_id"`
	TargetID       int     `json:"target_id"`
	Amount         float64 `json:"amount"`
}

// --- Dependency Injection Struct ---
// This is the missing piece causing your error.
// Instead of global variables, we hold dependencies inside this struct.
type TransferHandler struct {
	rdb       *redis.Client
	publisher message.Publisher
}

// ServeHTTP allows TransferHandler to act as a standard http.Handler
func (h *TransferHandler) ServeHTTP(w http.ResponseWriter, r *http.Request) {
	// --- DIAGRAM STEP: Listen For Requests ---
	if r.Method != http.MethodPost {
		http.Error(w, "Method not allowed", http.StatusMethodNotAllowed)
		return
	}

	// --- DIAGRAM STEP: Valid Request? ---
	var req TransferRequest
	if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
		http.Error(w, "Invalid JSON", http.StatusBadRequest)
		return
	}

	if req.Amount <= 0 || req.SourceID == req.TargetID || req.IdempotencyKey == "" {
		http.Error(w, "Invalid request parameters", http.StatusBadRequest)
		return
	}

	ctx := context.Background()

	// --- DIAGRAM STEP: Dupe Idem Key? (Redis) ---
	// Note: We use 'h.rdb' instead of a global variable
	exists, err := h.rdb.Exists(ctx, req.IdempotencyKey).Result()
	if err != nil {
		http.Error(w, "Internal Server Error", http.StatusInternalServerError)
		return
	}

	if exists > 0 {
		w.Header().Set("Content-Type", "application/json")
		w.WriteHeader(http.StatusOK)
		w.Write([]byte(`{"status": "duplicate_request_acknowledged"}`))
		return
	}

	// --- DIAGRAM STEP: Send job to RabbitMQ ---
	payload, err := json.Marshal(req)
	if err != nil {
		http.Error(w, "Internal Server Error", http.StatusInternalServerError)
		return
	}

	msg := message.NewMessage(watermill.NewUUID(), payload)

	// Note: We use 'h.publisher' here
	err = h.publisher.Publish(topicName, msg)

	// --- DIAGRAM STEP: RabbitMQ Publish OK? ---
	if err != nil {
		log.Printf("Failed to publish to RabbitMQ: %v", err)
		http.Error(w, "Service Unavailable", http.StatusServiceUnavailable)
		return
	}

	// --- DIAGRAM STEP: Write Key to Redis ---
	err = h.rdb.Set(ctx, req.IdempotencyKey, "processing", 24*time.Hour).Err()
	if err != nil {
		log.Printf("Warning: Failed to save idempotency key to Redis: %v", err)
	}

	// --- DIAGRAM STEP: 202 ACCEPTED ---
	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(http.StatusAccepted)
	w.Write([]byte(`{"status": "accepted", "message_id": "` + msg.UUID + `"}`))
}

func main() {
	// 1. Initialize Infrastructure
	rdb := initRedis()
	publisher := initWatermill()
	defer publisher.Close()

	// 2. Create the Handler with dependencies
	handler := &TransferHandler{
		rdb:       rdb,
		publisher: publisher,
	}

	// 3. Register the handler
	// We pass handler.ServeHTTP as the function
	http.HandleFunc("/transfer", handler.ServeHTTP)

	// 4. Start Server
	fmt.Println("Go API (Watermill Edition) listening on :8080...")
	if err := http.ListenAndServe(":8080", nil); err != nil {
		log.Fatal(err)
	}
}

// --- Infrastructure Setup ---

func initRedis() *redis.Client {
	rdb := redis.NewClient(&redis.Options{
		Addr: redisAddr,
	})
	if _, err := rdb.Ping(context.Background()).Result(); err != nil {
		log.Fatalf("Could not connect to Redis: %v", err)
	}
	return rdb
}

func initWatermill() message.Publisher {
	amqpConfig := amqp.NewDurableQueueConfig(amqpURI)
	publisher, err := amqp.NewPublisher(amqpConfig, watermill.NewStdLogger(false, false))
	if err != nil {
		log.Fatalf("Could not create Watermill publisher: %v", err)
	}
	return publisher
}
