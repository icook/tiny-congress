package db

import (
	"net/url"
	"path"
	"path/filepath"

	"github.com/google/uuid"
	"github.com/pkg/errors"
)

type NotFoundErr struct{ msg string }

func (n NotFoundErr) Error() string { return n.msg }

var (
	ErrObjectRulesNotFound         = NotFoundErr{"object rules not found"}
	ErrAttributeRulesNotFound      = NotFoundErr{"attribute rules not found"}
	ErrUniqIdentifierRulesNotFound = NotFoundErr{"unique identifier rules not found"}
	ErrUniqIdentifierNotFound      = NotFoundErr{"unique identifier not found"}
)

// StorageEngine provides a high level interface for interacting with objects
// in the store backed by a lower level KV interface.
type StorageEngine interface {
	GetObjectID(UIDStorageDetails) (uuid.UUID, error)
}

// UIDStorageDetails are the elements needed to construct a storage key path
// For fetching or setting objects to
type UIDStorageDetails struct {
	ObjectTypeName      string
	IdentifierName      string
	IdentifierLookupKey string
}

type ObjectEngine interface {
	GetRuleset(typeCode string) (ObjectRuleset, bool)
}

type ObjectRuleset interface {
	UniqueIdentifier(identifierName string) (UniqueIdentifierType, bool)
	AttributeType(attributeName string) (string, bool)
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
	store StorageEngine
	obj   ObjectEngine
	attr  AttributeEngine
}

func NewPersistenceLayer(store StorageEngine, obj ObjectEngine) (*PersistenceLayer, error) {
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

	// Validate the new data before we go any further
	attributeType, found := objectRules.AttributeType(req.AttributeName)
	if !found {
		return ErrAttributeRulesNotFound
	}
	attributeRules, found := p.attr.AttributeRuleset(attributeType)
	if !found {
		return ErrAttributeRulesNotFound
	}
	// Validate format of new value against attribute type system
	if err := attributeRules.MaySet(req.NewValue); err != nil {
		return errors.Wrap(err, "Invalid NewValue")
	}

	// Lookup the object
	identifierRules, found := objectRules.UniqueIdentifier(uid.IdentifierName)
	if !found {
		return ErrUniqIdentifierRulesNotFound
	}
	objID, err := p.store.GetObjectID(UIDStorageDetails{
		ObjectTypeName:      uid.ObjectTypeName,
		IdentifierName:      uid.IdentifierName,
		IdentifierLookupKey: identifierRules.DetermenisticKey(uid.Query),
	})
	if err != nil {
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

func (p PersistenceLayer) GetObjectID(req FetchObjectRequest) (Object, error) {
	uid, err := NewUID(req.URI)
	if err != nil {
		return Object{}, err
	}
	// Parse URI to determine the "type" of object we're looking for. Type is encoded in the URI.Sceme
	objectRules, found := p.obj.GetRuleset(uid.ObjectTypeName)
	if !found {
		return Object{}, ErrObjectRulesNotFound
	}

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
