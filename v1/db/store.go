package db

import (
	"encoding/json"
	"path"

	"github.com/google/uuid"
	"github.com/pkg/errors"
)

const (
	keyPrefixIdentifier = "/uid/"
	keyPrefixObjects    = "/obj/"
)

// StorageDriver is the data storage layer. Ideally I think I would like to be
// able to use Redis, postgres, leveldb, or raw json text files
type StorageDriver interface {
	WriteKey(key string, data []byte) error
	GetKey(key string) ([]byte, error)
	ErrIsNotFound(error) bool
}

// Store is an implementation of StorageEngine
type Store struct {
	d StorageDriver
}

func (s Store) objectPath(oid uuid.UUID) string {
	return path.Join(
		keyPrefixObjects,
		oid.String(),
	)
}

func (s Store) uidPath(det UIDStorageDetails) string {
	return path.Join(
		keyPrefixIdentifier,
		det.ObjectTypeName,
		det.IdentifierName,
		det.IdentifierLookupKey,
	)
}

// GetObjectID looks up an ObjectID via a rendered unique identifier type
func (s Store) GetObjectID(det UIDStorageDetails) (uuid.UUID, error) {
	key := s.uidPath(det)
	objIDRaw, err := s.d.GetKey(key)
	if s.d.ErrIsNotFound(err) {
		return uuid.UUID{}, ErrUniqIdentifierNotFound
	}
	if err != nil {
		return uuid.UUID{}, errors.WithStack(err)
	}
	return uuid.FromBytes(objIDRaw)
}

// GetObject grabs the raw k/v pairs stored for a given object UUID
func (s Store) GetObject(oid uuid.UUID) (map[string]string, error) {
	key := s.objectPath(oid)
	objRawJSON, err := s.d.GetKey(key)
	if s.d.ErrIsNotFound(err) {
		return nil, ErrUniqIdentifierNotFound
	}
	if err != nil {
		return nil, errors.WithStack(err)
	}
	var output = make(map[string]string)
	err = json.Unmarshal(objRawJSON, &output)
	if err != nil {
		return nil, errors.WithStack(err)
	}
	return output, nil
}
