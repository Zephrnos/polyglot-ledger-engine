package main

import (
	"bytes"
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"testing"
	"time"

	"github.com/ThreeDotsLabs/watermill"
	"github.com/ThreeDotsLabs/watermill/pubsub/gochannel"
	"github.com/alicebob/miniredis/v2"
	"github.com/redis/go-redis/v9"
	"github.com/stretchr/testify/assert"
)

func TestHandleTransfer_Success(t *testing.T) {
	// 1. MOCK REDIS
	// We spin up a fake Redis server in memory
	mr, err := miniredis.Run()
	assert.NoError(t, err)
	defer mr.Close()

	rdb := redis.NewClient(&redis.Options{
		Addr: mr.Addr(),
	})

	// 2. MOCK WATERMILL (RabbitMQ)
	// We use GoChannel, which acts like a queue but just uses Go channels
	logger := watermill.NewStdLogger(false, false)
	pubSub := gochannel.NewGoChannel(gochannel.Config{}, logger)

	// 3. SUBSCRIBE to the fake queue so we can verify the message arrived
	messages, err := pubSub.Subscribe(context.Background(), "transactions")
	assert.NoError(t, err)

	// 4. SETUP HANDLER
	handler := &TransferHandler{
		rdb:       rdb,
		publisher: pubSub, // We pass the in-memory publisher
	}

	// 5. CREATE REQUEST
	payload := TransferRequest{
		IdempotencyKey: "test-key-123",
		SourceID:       1,
		TargetID:       2,
		Amount:         100.50,
	}
	body, _ := json.Marshal(payload)
	req := httptest.NewRequest(http.MethodPost, "/transfer", bytes.NewBuffer(body))

	// We create a Recorder to capture the response
	w := httptest.NewRecorder()

	// 6. EXECUTE
	handler.ServeHTTP(w, req)

	// 7. ASSERTIONS
	// Check HTTP Code
	assert.Equal(t, http.StatusAccepted, w.Code)

	// Check Response Body
	var response map[string]string
	json.Unmarshal(w.Body.Bytes(), &response)
	assert.Equal(t, "accepted", response["status"])

	// Check Redis: Ensure key was set
	val, err := mr.Get("test-key-123")
	assert.NoError(t, err)
	assert.Equal(t, "processing", val)

	// Check RabbitMQ: Ensure message was published
	select {
	case msg := <-messages:
		// Verify the payload sent to the queue matches what we sent
		var receivedReq TransferRequest
		json.Unmarshal(msg.Payload, &receivedReq)
		assert.Equal(t, 100.50, receivedReq.Amount)
		msg.Ack()
	case <-time.After(time.Second):
		t.Fatal("Expected message to be published to queue, but none received")
	}
}

func TestHandleTransfer_DuplicateIdempotency(t *testing.T) {
	// Setup Mocks
	mr, _ := miniredis.Run()
	defer mr.Close()
	rdb := redis.NewClient(&redis.Options{Addr: mr.Addr()})

	// Pre-fill Redis with the key to simulate a duplicate
	mr.Set("duplicate-key", "processing")

	handler := &TransferHandler{
		rdb:       rdb,
		publisher: nil, // We don't even need a publisher here, code should return early
	}

	// Create Request
	payload := TransferRequest{
		IdempotencyKey: "duplicate-key",
		SourceID:       1,
		TargetID:       2,
		Amount:         50.00,
	}
	body, _ := json.Marshal(payload)
	req := httptest.NewRequest(http.MethodPost, "/transfer", bytes.NewBuffer(body))
	w := httptest.NewRecorder()

	// Execute
	handler.ServeHTTP(w, req)

	// Assert: Should be 200 OK (not 202 Accepted)
	assert.Equal(t, http.StatusOK, w.Code)
	assert.Contains(t, w.Body.String(), "duplicate_request_acknowledged")
}
