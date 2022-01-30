package mem

import (
	"errors"

	"github.com/icook/tiny-congress/db"
)

var _ db.StorageDriver = Store{}

type storeObj struct {
	data []byte
}

// Store implements a minimal in memory StorageDriver for unit testing
type Store struct {
	store map[string]storeObj
}

func NewMemStore() *Store {
	return &Store{
		store: map[string]storeObj{},
	}
}

func (m Store) WriteKey(key string, data []byte) error {
	m.store[key] = storeObj{
		data: data,
	}
	return nil
}

func (m Store) GetKey(key string) ([]byte, error) {
	obj, found := m.store[key]
	if !found {
		return nil, errors.New("not found")
	}
	return obj.data, nil
}
