package model

import (
	"errors"
	"sync"
)

// PollImpl implements the Poll interface
type PollImpl struct {
	Topic      string
	Dimensions []string
	CreatorID  string
	Votes      map[string]map[string]float64
	mu         sync.RWMutex
	closed     bool
}

func NewPoll(topic string, dimensions []string, creatorID string) *PollImpl {
	return &PollImpl{
		Topic:      topic,
		Dimensions: dimensions,
		CreatorID:  creatorID,
		Votes:      make(map[string]map[string]float64),
	}
}

func (p *PollImpl) CreatePoll(topic string, dimensions []string, creatorID string) error {
	p.mu.Lock()
	defer p.mu.Unlock()

	if p.Topic != "" {
		return errors.New("poll already initialized")
	}

	p.Topic = topic
	p.Dimensions = dimensions
	p.CreatorID = creatorID
	return nil
}

func (p *PollImpl) CastVote(voterID string, dimension string, value float64) error {
	p.mu.Lock()
	defer p.mu.Unlock()

	if p.closed {
		return errors.New("poll is closed")
	}

	validDimension := false
	for _, d := range p.Dimensions {
		if d == dimension {
			validDimension = true
			break
		}
	}
	if !validDimension {
		return errors.New("invalid dimension")
	}

	if p.Votes[voterID] == nil {
		p.Votes[voterID] = make(map[string]float64)
	}
	p.Votes[voterID][dimension] = value
	return nil
}

func (p *PollImpl) GetResults() (map[string]float64, error) {
	p.mu.RLock()
	defer p.mu.RUnlock()

	results := make(map[string]float64)
	counts := make(map[string]int)

	for _, votes := range p.Votes {
		for dimension, value := range votes {
			results[dimension] += value
			counts[dimension]++
		}
	}

	for dimension, total := range results {
		if counts[dimension] > 0 {
			results[dimension] = total / float64(counts[dimension])
		}
	}

	return results, nil
}

func (p *PollImpl) ClosePoll() error {
	p.mu.Lock()
	defer p.mu.Unlock()
	p.closed = true
	return nil
}
