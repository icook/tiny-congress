package token

import "github.com/google/uuid"

// Grants are how tokens are given to users. Grants are intended to be quick
// and easy to compute, and are the source of truth for which identities hold
// what privileges in a given network
type Grant interface {
	Quantity() float64
	TokenTypeName() string
	Identity() uuid.UUID
}
