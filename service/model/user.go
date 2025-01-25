package model

import (
	"crypto/ed25519"
	"errors"
	"sync"
)

// UserImpl implements the User interface
type UserImpl struct {
	PublicKey        ed25519.PublicKey
	IdentityVerified bool
	TrustScores      map[string]map[string]float64
	mu               sync.RWMutex
}

func NewUser() *UserImpl {
	return &UserImpl{
		TrustScores: make(map[string]map[string]float64),
	}
}

func (u *UserImpl) Register(publicKey string, identityVerification bool) error {
	u.mu.Lock()
	defer u.mu.Unlock()

	// In a real implementation, validate the public key format
	if len(publicKey) == 0 {
		return errors.New("invalid public key")
	}

	// Store as bytes
	pkBytes := []byte(publicKey)
	if len(pkBytes) != ed25519.PublicKeySize {
		return errors.New("invalid public key size")
	}

	u.PublicKey = pkBytes
	u.IdentityVerified = identityVerification
	return nil
}

func (u *UserImpl) UpdateTrustScore(dimension, subDimension string, value float64) error {
	u.mu.Lock()
	defer u.mu.Unlock()

	if value < 0 || value > 1 {
		return errors.New("trust score must be between 0 and 1")
	}

	if u.TrustScores[dimension] == nil {
		u.TrustScores[dimension] = make(map[string]float64)
	}
	u.TrustScores[dimension][subDimension] = value
	return nil
}

func (u *UserImpl) GetTrustScores() (map[string]map[string]float64, error) {
	u.mu.RLock()
	defer u.mu.RUnlock()

	// Return a deep copy to prevent external modification
	scores := make(map[string]map[string]float64)
	for dim, subDims := range u.TrustScores {
		scores[dim] = make(map[string]float64)
		for subDim, value := range subDims {
			scores[dim][subDim] = value
		}
	}
	return scores, nil
}
