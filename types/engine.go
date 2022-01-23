package types

// This defines the ruleset implementation. _How_ we decide to apply a change
// or not depends on implementations here, effectively the gate functions
// between an unauthorized "request" and an applied operation (read: change or
// action).
// The end game goal would be for this to consume data from the database in the
// form of an upgradeable EVM bytecode bundle (think of this as the "runtime"
// of the given network. core logic. hard fork) and more easily changed runtime
// configuration parameters, which are merely network consensus somewhere in
// the database. Exactly how these two tie together is the real magic that
// needs to be done right
type ObjectEngine struct {
}

// func
