package model

import "sync"

// RoomImpl implements the Room interface
type RoomImpl struct {
	activePolls []Poll
	rules       []Rule
	mu          sync.RWMutex
}

func NewRoom() *RoomImpl {
	return &RoomImpl{
		activePolls: make([]Poll, 0),
		rules:       make([]Rule, 0),
	}
}

func (r *RoomImpl) AddPoll(poll Poll) error {
	r.mu.Lock()
	defer r.mu.Unlock()
	r.activePolls = append(r.activePolls, poll)
	return nil
}

func (r *RoomImpl) RotatePolls() error {
	r.mu.Lock()
	defer r.mu.Unlock()

	// Close all active polls
	for _, poll := range r.activePolls {
		poll.ClosePoll()
	}
	r.activePolls = make([]Poll, 0)
	return nil
}

func (r *RoomImpl) GetActivePolls() ([]Poll, error) {
	r.mu.RLock()
	defer r.mu.RUnlock()
	return r.activePolls, nil
}

func (r *RoomImpl) SetRules(rules []Rule) error {
	r.mu.Lock()
	defer r.mu.Unlock()
	r.rules = rules
	return nil
}
