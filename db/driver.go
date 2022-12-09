package db

import (
	"fmt"
)

// The data storage layer. Ideally I think I would like to be able to use Redis, postgres, leveldb, or raw json text files
type StorageDriver interface {
	WriteKey(key string, data []byte) error
	GetKey(key string) ([]byte, error)
}

type TypeEngine interface {
	GetRuleset(typeCode string) TypeRuleset
}

type TypeRuleset interface {
	MaySet(newValue string) error
}

type PersistenceLayer struct {
	d StorageDriver
	t TypeEngine
}

func NewPersistenceLayer(d StorageDriver, t TypeEngine) (*PersistenceLayer, error) {
	return &PersistenceLayer{d: d, t: t}, nil
}

type Identifier interface {
	Pairs() map[string]string
	Key() string
	// TODO: I think we might want "identifier types" somehow? Reusable?
	Name() string
}

type Object interface {
	Identifiers() []Identifier
}

func (p PersistenceLayer) UpdateKey(identifier Identifier, key string, value string, valueType string) error {
	ruleset := p.t.GetRuleset(valueType)
	// Validate format of new value against type system
	if err := ruleset.MaySet(value); err != nil {
		return err
	}
	// Should produce a name like 'file.sha#42c79fd316123b7acfc99d7e0c3bdbe0d0df144cd7b48fb11e2ba5c8699dcdb0'
	// where "sha" is the identifier "type" and "file"
	keyName := fmt.Sprintf("%s.%s#%s", valueType, identifier.Name(), identifier.Key())
	if err := p.d.WriteKey(keyName, []byte(value)); err != nil {
		return err
	}
	return nil
}

// TODO: consider that perhaps valueType should be an Identifier?
func (p PersistenceLayer) FetchKey(identifier Identifier, valueType string) (Object, error) {
	return nil, nil
}
