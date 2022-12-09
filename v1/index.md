# TODO

- `db/driver.go:PersistenceLayer`: add support for lists insert and (pre|ap)pend. Consider other datatypes, perhaps with APIs similar to redis.
- Try and get 1 minimally functional unit
- I need to add another layer between the PersistenceLayer and the StorageDriver. Something that maps higher level operations (object retrieval, etc) to key/val operations.
- NotFoundErr from StorageEngine should wrap lower layer error, since that will frequently be informative

# Concepts

This is a living document where I try to explain how things fit together. I will inevitable discover large problems here and heavily revise and refactor.

# Authorship

Goal of this doc is to read as a manual to a prospective writter of congressional logic.

# Prototype 1 Aims

Ignore networking, p2p, all of that. We're going to build an extremely robust 
filing system for zettelkasten as a way to mature concepts of the storage engine.

It is essential that this is powerful and composable, and it can be iterated on 
as a personal data management/indexining/filing/curation system. We want to be
a configurable cataloging system, and then build elections on top of these
abstractions.

# Imagined Evolution

After the base data layer and data browser is working in dictator mode we would
like to evolve our congress to rely on multiple sources of authority.

This requires introducing several new concepts that build on top of our typed
data layer.

## MandateGrant

Every authority network grants authority to

```go
// The UUID of a given network identity
type IdentityUID uuid.UUID
// TokenType is a unique type string code for a given token
type TokenType string
// TokenNamespace separates tokens by their use. Runtime reserves the
// Namespaces "authority" and "influence" for builtin survey and election logic
type TokenNamespace string
// MandateGrants are network state that gives Identities the ability to do
// things in the non-dictatorial network
type MandateGrant struct {
    TokenType TokenType
    TokenNamespace TokenNamespace
    AuthorizedIdentities map[IdentityUID]float64
}
// A given Identity may be granted Authority. This would be an input to
// election gate functions.
type Authority map[TokenType]float64
// A given Identity may be granted tokens on the "influence" namespace. These
// are passed to Attribute
type Influence map[TokenType]float64
```

## Identity

Every authority 

## Surveys

## Elections

## PersistenceLayer

This is how we all data in the network is stored. It is just some simple
abstractions on top of a key value store, so it can run just about anywhere.

Imagined StorageDriver implementations:

- JSON files, perhaps encrypted and committed to git, for important secrets (think pass) 
- JSON files, perhaps served directly from an S3 bucket that a website uses for rendering. Think asset prices, etc.
- LevelDB if you need high capacity with good read/write throughput
- Postgresql, if you need to integrate the network state with another application, perhaps as part of larger transactions
- Redis, if you need high throughput but your state is ephemeral

The goal is to allow authors their choice depending on their needs. I'll admit there are downsides to this design, but we think the tradeoffs in portability for backend are worth it.

Interface

```go
// A unix style path tree
type AttributeName string
type ValueType string
type PersistenceReader interface {
    FetchKey(UniqueIdentifier, _ ValueType) (Object, error)
}
type Value interface {
    Type() ValueType
    String() string
}
type PersistenceWriter interface {
    PersistenceReader
    UpdateKey(UniqueIdentifier, AttributeName, Value)
}
```

## ObjectSchema

Objects may not be nested, but they may reference other objects. References and ReferenceLists are a builtin datatype, and references may point to any object in the graph using any configured UniqueIdentifier. 

:insp: ObjectSchemas are very similar to Kubernetes CRDs. Perhaps I should just use them?

- every attribute should have last updated time...

Here's an example schema of a system for tracking files that are snapshots of a
website across multiple machines.

```yaml
objectType: Website
attributes:
    - name: url
      type:
          name: url
          schemas: ["http", "https"]
uids:
    - name: url
      attributes: ["url"]

objectType: File
attributes:
    - name: sha256_hex
      type: byte
      typeParams:
          maxLength: 32
uids:
    - attributes: ["sha256_hex"]

objectType: FileLocations
attributes:
    - name: path
      type:
          name: path
    - name: machine
      type:
          name: string
    - name: file
      type: reference
      typeParams:
          objectType: File
          uid: sha256_hex
uids:
    - attributes: ["machine", "path"]

objectType: FileTags
attributes:
    - name: tags
      dataType: list:string
    - name: tags
      dataType: list:string
uids:
    - attributes: ["machine", "path"]

objectType: WebsiteSnapshot
attributes:
    - name: website
      dataType: ref:Website@url
    - name: file
      dataType: ref:File@sha256_hex
```

## DataTypes

DataTypes are implementated as an implementation of:

### State reducers

These are the target of `election ratification clauses` in the "long term vision," but for now simply populate some buttons on a web UI that allow a single user "dictator" to mutate state at will.

### ? Builtins

What are the builtins we want? Any how to imlpement polymorphic types? How to implement types with length?

### TypeRegistry

Is a component that holds all registered type implementations and provides entry points into calling their commands.

## Prototype

Prototype "`zettelkasten-db`" will have a simple react UI that hits an unauthenticated local service over http. UI will provide simple text search of objects in the store, display of a particular object and it git history.

Objects will all have a key "type" that must map to a registered type.
