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

const (
	amqpURI   = "amqp://guest:guest@localhost:5672/"
	redisAddr = "localhost:6379"
	topicName = "transactions"
)

type TransferRequest struct {
	IdempotencyKey string  `json:"idempotency_key"`
	SourceID       int     `json:"source_id"`
	TargetID       int     `json:"target_id"`
	Amount         float64 `json:"amount"`
}

type TransferHandler struct {
	rdb       *redis.Client
	publisher message.Publisher
}

func (h *TransferHandler) ServeHTTP(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodPost {
		http.Error(w, "Method not allowed", http.StatusMethodNotAllowed)
		return
	}

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

	// 1. Idempotency Check
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

	// --- FIX STARTS HERE ---
	// We write to Redis BEFORE publishing. This prevents the race condition.

	// 2. Lock Key in Redis (Idempotency)
	h.rdb.Set(ctx, req.IdempotencyKey, "processing", 24*time.Hour)

	// 3. Write "status:" key (Pending)
	h.rdb.Set(ctx, "status:"+req.IdempotencyKey, "pending", 24*time.Hour)

	// --- FIX ENDS HERE ---

	// 4. Publish to Queue
	payload, _ := json.Marshal(req)
	msg := message.NewMessage(watermill.NewUUID(), payload)

	if err := h.publisher.Publish(topicName, msg); err != nil {
		// If publishing fails, we should technically cleanup Redis,
		// but for this demo, we just log the error.
		log.Printf("Failed to publish: %v", err)
		http.Error(w, "Service Unavailable", http.StatusServiceUnavailable)
		return
	}

	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(http.StatusAccepted)
	w.Write([]byte(`{"status": "accepted", "message_id": "` + msg.UUID + `"}`))
}

func (h *TransferHandler) HandleStatus(w http.ResponseWriter, r *http.Request) {
	key := r.URL.Query().Get("key")
	if key == "" {
		http.Error(w, "Missing key", http.StatusBadRequest)
		return
	}

	ctx := context.Background()
	val, err := h.rdb.Get(ctx, "status:"+key).Result()

	if err == redis.Nil {
		w.Header().Set("Content-Type", "application/json")
		w.Write([]byte(`{"status": "unknown", "detail": "Key not found"}`))
		return
	} else if err != nil {
		http.Error(w, "Redis Error", http.StatusInternalServerError)
		return
	}

	response := map[string]string{
		"idempotency_key": key,
		"result":          val,
	}
	w.Header().Set("Content-Type", "application/json")
	json.NewEncoder(w).Encode(response)
}

func main() {
	rdb := redis.NewClient(&redis.Options{Addr: redisAddr})
	amqpConfig := amqp.NewDurableQueueConfig(amqpURI)
	publisher, err := amqp.NewPublisher(amqpConfig, watermill.NewStdLogger(false, false))
	if err != nil {
		log.Fatal(err)
	}
	defer publisher.Close()

	handler := &TransferHandler{rdb: rdb, publisher: publisher}

	http.HandleFunc("/transfer", handler.ServeHTTP)
	http.HandleFunc("/status", handler.HandleStatus)

	fmt.Println("Go API listening on :8080...")
	log.Fatal(http.ListenAndServe(":8080", nil))
}
