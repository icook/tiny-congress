package election

import (
	"time"

	"github.com/google/uuid"
)

type TokenTypeCode string
type TokenType interface {
	Code() TokenTypeCode
	Description() string
}

type Identity interface {
	ID() uuid.UUID
}

type Ballot interface {
	Identity() Identity
	DeclaredTime() time.Time
	TokensCast() map[TokenTypeCode]float64
}

// Could be stored in the database
type RoundRuleset interface {
	// Our database unique identifier
	Name() string

	// These are the types of token that might materially influence the outcome
	// of the election. In practice users can still include other tokens in
	// their ballots, they will simply be ignored and filtered out of the input
	// to the RoundRuleset implementations
	RelevantTokenTypes() []string

	// Code implemented in wasm, stored in our db at something like
	// "round_ruleset.simple_majority"
	IsRatified([]Ballot) bool
	IsRejected([]Ballot) RetryOption
	IsExtended([]Ballot) RoundExtension
}

type ElectionConfig interface {
	// Our database unique identifier
	Name() string

	Rounds() []RoundConfig
}

type RoundConfig struct {
	RoundRulesetName string
	BaseDuration     time.Duration
}

type ElectionStatus string

const (
	ElectionStatusInProgress ElectionStatus = "in_progress"
	ElectionStatusRatified   ElectionStatus = "ratified"
	ElectionStatusRejected   ElectionStatus = "rejected"
)

type Election interface {
	Rounds() []Round
	Status() ElectionStatus
}

// Elections are run as a series of rounds. Rounds collect tokens until
type Round interface {
	ID() string
	Ruleset() RoundExtension
	Ballots() []Ballot
	TokensCast() map[TokenTypeCode]float64
	StartTime() time.Time
}

// How long the raised topic must hold to the decisions of this election.
// Motions to revisit this election topic will be disallowed until RetryAfter()
// time.
type RetryOption interface {
	RetryAfter() time.Duration
}

// How much longer the round will be extended for additional ballots
type RoundExtension interface {
	Duration() time.Duration
}
