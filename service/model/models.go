package tinycongress

import (
	"errors"
	"sync"
	"time"
)

// Attestation represents a trust attestation between entities
type Attestation struct {
	AttestorID string
	SubjectID  string
	Dimension  string
	Value      float64
	Timestamp  time.Time
	Signature  []byte
}

// TrustGraphImpl implements the TrustGraph interface
type TrustGraphImpl struct {
	attestations []Attestation
	mu           sync.RWMutex
}

func NewTrustGraph() *TrustGraphImpl {
	return &TrustGraphImpl{
		attestations: make([]Attestation, 0),
	}
}

func (g *TrustGraphImpl) AddAttestation(attestorID, subjectID, dimension string, value float64, signature []byte) error {
	if value < 0 || value > 1 {
		return errors.New("trust value must be between 0 and 1")
	}

	attestation := Attestation{
		AttestorID: attestorID,
		SubjectID:  subjectID,
		Dimension:  dimension,
		Value:      value,
		Timestamp:  time.Now(),
		Signature:  signature,
	}

	g.mu.Lock()
	defer g.mu.Unlock()
	g.attestations = append(g.attestations, attestation)
	return nil
}

func (g *TrustGraphImpl) VerifyAttestation(attestation Attestation) (bool, error) {
	// In a real implementation, this would verify the cryptographic signature
	// For now, we'll do basic validation
	if attestation.Value < 0 || attestation.Value > 1 {
		return false, errors.New("invalid trust value")
	}
	return true, nil
}

func (g *TrustGraphImpl) ConvergeTrustGraph() (map[string]map[string]float64, error) {
	g.mu.RLock()
	defer g.mu.RUnlock()

	// Simple averaging of attestations per dimension
	results := make(map[string]map[string]float64)
	counts := make(map[string]map[string]int)

	for _, att := range g.attestations {
		if results[att.SubjectID] == nil {
			results[att.SubjectID] = make(map[string]float64)
			counts[att.SubjectID] = make(map[string]int)
		}
		results[att.SubjectID][att.Dimension] += att.Value
		counts[att.SubjectID][att.Dimension]++
	}

	// Calculate averages
	for subjectID, dimensions := range results {
		for dimension, total := range dimensions {
			count := counts[subjectID][dimension]
			if count > 0 {
				results[subjectID][dimension] = total / float64(count)
			}
		}
	}

	return results, nil
}
