package db

import (
	"errors"
	"net/url"
	"path"
	"path/filepath"
)

const (
	storagePrefixObjects = "/objects/"
)

type NotFoundErr struct{ msg string }

func (n NotFoundErr) Error() string { return n.msg }

var (
	ErrObjectRulesNotFound         = NotFoundErr{"object rules not found"}
	ErrAttributeRulesNotFound      = NotFoundErr{"attribute rules not found"}
	ErrUniqIdentifierRulesNotFound = NotFoundErr{"unique identifier rules not found"}
)

// The data storage layer. Ideally I think I would like to be able to use Redis, postgres, leveldb, or raw json text files
type StorageDriver interface {
	WriteKey(key string, data []byte) error
	GetKey(key string) ([]byte, error)
}

type ObjectEngine interface {
	GetRuleset(typeCode string) (ObjectRuleset, bool)
}

type ObjectRuleset interface {
	UniqueIdentifier(typeCode string) UniqueIdentifierType
	Attributes() []string
}
type AttributeEngine interface {
	AttributeRuleset(typeCode string) (AttributeRuleset, bool)
}

// AttributeRuleset designates data formatting and subdividing logic
type AttributeRuleset interface {
	MaySet(newValue string) error
	SubAttributes() map[string]string
}

// UniqueIdentifierType are unique keys specified by the type. They may be a composite
// of multiple attributes For now we're just doing the "everything is a string"
// hack. This is a prototype... Typing system will give power to the data type
// dynamically.
type UniqueIdentifierType interface {
	// DetermenisticKey must map to the same string for the same input.
	// Key is used for datastore lookup path operations
	DetermenisticKey(IdentifierQuery) string
}

// PersistenceLayer implements the read and write api for election ratification
// functions to call. It is how network state mutates upon a successful
// election result.
type PersistenceLayer struct {
	store StorageDriver
	obj   ObjectEngine
	attr  AttributeEngine
}

func NewPersistenceLayer(store StorageDriver, obj ObjectEngine) (*PersistenceLayer, error) {
	return &PersistenceLayer{
		store: store,
		obj:   obj,
	}, nil
}

type IdentifierQuery url.Values

func (q IdentifierQuery) Get(key string) string {
	return q.Get(key)
}

type UID struct {
	ObjectTypeName string
	IdentifierName string
	// Query gets passed to the Attribute implementation for processing?
	Query IdentifierQuery
}

func NewUID(u url.URL) (*UID, error) {
	pathParts := filepath.SplitList(u.Path)
	if len(pathParts) < 0 {
		return nil, errors.New("malformed uri: no attributeName in path")
	}
	return &UID{
		ObjectTypeName: u.Scheme,
		IdentifierName: pathParts[0],
		Query:          IdentifierQuery(u.Query()),
	}, nil
}

type UpdateKeyRequest struct {
	URI           url.URL `json:"uri"`
	AttributeName string  `json:"attribute_name"`
	NewValue      string  `json:"new_value"`
}

func (p PersistenceLayer) UpdateKey(req UpdateKeyRequest) error {
	uid, err := NewUID(req.URI)
	if err != nil {
		return err
	}
	// Parse URI to determine the "type" of object we're looking for. Type is encoded in the URI.Sceme
	objectRules, found := p.obj.GetRuleset(uid.ObjectTypeName)
	if !found {
		return ErrObjectRulesNotFound
	}
	attributeRules, found := objectRules.AttributeRuleset(req.AttributeName)
	if !found {
		return ErrAttributeRulesNotFound
	}
	// Validate format of new value against attribute type system
	if err := attributeRules.MaySet(req.NewValue); err != nil {
		return errors.Wrap(err, "Invalid NewValue")
	}
	identifierRules, found := objectRules.UniqueIdentifier(uid.IdentifierName)
	if !found {
		return ErrUniqIdentifierRulesNotFound
	}
	storageKey := path.Join(storagePrefixObjects, attributeStorageKeyer(objectStorageKeyerParams{
		ObjectTypeName:      uid.ObjectTypeName,
		IdentifierName:      uid.IdentifierName,
		IdentifierLookupKey: identifierRules.DetermenisticKey(uid.Query),
		AttributeName:       req.AttributeName,
	}))
	if err := p.store.WriteKey(storageKey, []byte(req.NewValue)); err != nil {
		return err
	}
	return nil
}

// Pulled out for easier plugging/extraction later
type objectStorageKeyerParams struct {
	ObjectTypeName      string
	IdentifierName      string
	IdentifierLookupKey string
	AttributeName       string
}

// Should produce a path like '/file/sha/42c79fd316123b7acfc99d7e0c3bdbe0d0df144cd7b48fb11e2ba5c8699dcdb0/size'
func attributeStorageKeyer(params objectStorageKeyerParams) string {
	return path.Join(params.ObjectTypeName, params.IdentifierName, params.IdentifierLookupKey, params.AttributeName)
}

type FetchObjectRequest struct {
	URI url.URL `json:"uri"`
}

func (p PersistenceLayer) FetchObject(req FetchObjectRequest) (Object, error) {
	uid, err := NewUID(req.URI)
	if err != nil {
		return Object{}, err
	}
	// Parse URI to determine the "type" of object we're looking for. Type is encoded in the URI.Sceme
	objectRules := p.obj.GetRuleset(uid.ObjectTypeName)
	idLogic := objectRules.UniqueIdentifier(uid.IdentifierName)
	idLookupKey := idLogic.DetermenisticKey(uid.Query)

	// Lookup every sub-attribute that the type supports
	var rawAttrs = make(map[string]string)
	for _, attrName := range objectRules.Attributes() {
		storageKey := attributeStorageKeyer(objectStorageKeyerParams{
			ObjectTypeName:      uid.ObjectTypeName,
			IdentifierName:      uid.IdentifierName,
			IdentifierLookupKey: idLookupKey,
			AttributeName:       attrName,
		})
		attributeValue, err := p.store.GetKey(storageKey)
		if err != nil {
			return Object{}, err
		}
		rawAttrs[attrName] = string(attributeValue)
	}
	return Object{
		p:        p,
		rawAttrs: rawAttrs,
	}, nil
}

type Object struct {
	p        PersistenceLayer
	rawAttrs map[string]string
}

func (o Object) GetAttribute(name string) (Attribute, error) {
	rawAttr, found := o.rawAttrs[name]
	if !found {
		return Attribute{}, errors.New("attribute value not found")
	}
	attrRuleset, found := o.p.attr.AttributeRuleset(name)
	if !found {
		return Attribute{}, errors.New("attribute ruleset not found")
	}
	return Attribute{
		Name:        name,
		RawValue:    rawAttr,
		attrRuleset: attrRuleset,
		// Parts:
	}, nil
}

type Attribute struct {
	Name     string
	RawValue string

	attrRuleset AttributeRuleset
}

// Not sure if this is crazy...
// func (a Attribute) Parts() map[string]Attribute {
// 	return nil
// }
