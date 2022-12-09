package user

import "github.com/google/uuid"

// TODO...
type Ballot struct {
	Signature  []byte
	SigningKey uuid.UUID
}

type Elector interface {
	CastBallot(Ballot)
}
