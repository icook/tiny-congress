package types

import (
	"sort"

	"github.com/etnz/logfmt"

	"github.com/icook/tiny-congress/db"
)

// This defines the object ruleset implementation. _How_ we decide to apply a
// change or not depends on implementations here, effectively the gate
// functions between an unauthorized "request" and an applied operation (read:
// change or action). The end game goal would be for this to consume data from
// the database in the form of an upgradeable EVM bytecode bundle (think of
// this as the "runtime" of the given network. core logic. hard fork) and more
// easily changed runtime configuration parameters, which are merely network
// consensus somewhere in the database. Exactly how these two tie together is
// the real magic that needs to be done right
type ObjectEngine struct {
	rules map[string]ObjectRuleset
}

func (e ObjectEngine) GetRuleset(typeCode string) (ObjectRuleset, bool) {
	rules, found := e.rules[typeCode]
	return rules, found
}

// Implementation of `db/driver.go:ObjectRuleset`
type ObjectRuleset struct {
	identifiers map[string]UniqueIdentifierType
}

func (o ObjectRuleset) UniqueIdentifier(typeCode string) (UniqueIdentifierType, bool) {
	rules, found := o.identifiers[typeCode]
	return rules, found
}

type UniqueIdentifierType struct {
	attributeNames []string
}

func NewUniqueIdentifierType(attributeNames []string) UniqueIdentifierType {
	sort.Strings(attributeNames)
	return UniqueIdentifierType{
		attributeNames: attributeNames,
	}
}

func (u UniqueIdentifierType) DetermenisticKey(query db.IdentifierQuery) string {
	rec := logfmt.Rec()
	for _, attributeName := range u.attributeNames {
		attrValue := query.Get(attributeName)
		rec = rec.Q(attributeName, attrValue)
	}
	return rec.String()
}
