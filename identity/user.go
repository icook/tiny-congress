package identity

import (
	"time"

	"github.com/google/uuid"
)

type IdentityNetwork struct {
	// Users are immutable.
	Users         map[UserID]User
	UserKeychains map[UserID]UserKeychain
}

type User struct {
	ID          UserID
	FirstKnown  time.Time
	FoundingKey AuthorizedKey
	Founding    Authorization
}

type UserKeychain struct {
	Authorizations []KeyAuthorization
	Revokations    []KeyRevokation
}

func (u UserKeychain) AuthorizedKeys() []AuthorizedKey {
	panic("implement me")
}

type NetworkID uuid.UUID

type Network struct {
	ID NetworkID
}

type UserID uuid.UUID

// UserGrant is what gives users permissions on a given network
// TODO: Consider if this should have a UUID ID as well?
type UserGrant struct {
	UserID        UserID
	NetworkID     NetworkID
	TokenType     string
	TokenQuantity float64
}

type KeyType string
type KeyID uuid.UUID

// AuthorizedKey is a Key that has a valid KeyAuthorization and no known KeyRevokation
type AuthorizedKey struct {
	ID        KeyID
	UserID    uuid.UUID
	KeyType   KeyType
	PublicKey []byte
}

// KeyAuthorization is an object that authorizes a User key other than the
// founding key
type KeyAuthorization struct {
	Authorization
	AuthorizedKey KeyID
}

type KeyRevokation struct {
	Authorization
	RevokedKey KeyID
}

type Authorization struct {
	SigningKey KeyID
	Signature  []byte
	SignedAt   time.Time
}

// KeyVerifier are registered by KeyType
type KeyVerifier interface {
	Verify(signature []byte, publicKey []byte)
}
