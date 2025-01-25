package model

import (
	"sync"
	"testing"
)

type MockPoll struct {
	closed bool
}

func (m *MockPoll) ClosePoll() {
	m.closed = true
}

type Poll interface {
	ClosePoll()
}

type Rule struct{}

func TestNewRoom(t *testing.T) {
	room := NewRoom()
	if room == nil {
		t.Error("Expected new room to be created")
	}
	if len(room.activePolls) != 0 {
		t.Error("Expected no active polls in new room")
	}
	if len(room.rules) != 0 {
		t.Error("Expected no rules in new room")
	}
}

func TestAddPoll(t *testing.T) {
	room := NewRoom()
	poll := &MockPoll{}
	err := room.AddPoll(poll)
	if err != nil {
		t.Errorf("Unexpected error: %v", err)
	}
	if len(room.activePolls) != 1 {
		t.Error("Expected one active poll")
	}
}

func TestRotatePolls(t *testing.T) {
	room := NewRoom()
	poll := &MockPoll{}
	room.AddPoll(poll)
	err := room.RotatePolls()
	if err != nil {
		t.Errorf("Unexpected error: %v", err)
	}
	if len(room.activePolls) != 0 {
		t.Error("Expected no active polls after rotation")
	}
	if !poll.closed {
		t.Error("Expected poll to be closed after rotation")
	}
}

func TestGetActivePolls(t *testing.T) {
	room := NewRoom()
	poll := &MockPoll{}
	room.AddPoll(poll)
	activePolls, err := room.GetActivePolls()
	if err != nil {
		t.Errorf("Unexpected error: %v", err)
	}
	if len(activePolls) != 1 {
		t.Error("Expected one active poll")
	}
}

func TestSetRules(t *testing.T) {
	room := NewRoom()
	rules := []Rule{{}, {}}
	err := room.SetRules(rules)
	if err != nil {
		t.Errorf("Unexpected error: %v", err)
	}
	if len(room.rules) != 2 {
		t.Error("Expected two rules to be set")
	}
}
