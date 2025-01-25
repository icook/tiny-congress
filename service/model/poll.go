package model

// Poll represents a voting session
type Poll interface {
	CreatePoll(topic string, dimensions []string, creatorID string) error
	CastVote(voterID string, dimension string, value float64) error
	GetResults() (map[string]float64, error)
	ClosePoll() error
}

// Rule represents a governance rule in a room
type Rule struct {
	Name        string
	Description string
	// Evaluates if an action is allowed based on context
	Evaluate func(context map[string]interface{}) bool
	// Weight of this rule (0-1) for conflict resolution
	Weight float64
}
