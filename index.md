# TODO

- `db/driver.go:PersistenceLayer`: add support for lists insert and (pre|ap)pend. Consider other datatypes, perhaps with APIs similar to redis.
- Try and get 1 minimally functional unit

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
object_type: Website
attributes:
    - name: url
      data_type: uri:url
unique_keys:
    - attributes: ["url"]

object_type: File
attributes:
    - name: sha256_hex
      data_type: [32]byte
unique_keys:
    - attributes: ["sha256_hex"]

object_type: FileLocations
attributes:
    - name: path
      data_type: unix_path
    - name: machine
      data_type: string
    - name: file
      data_type: ref:File@sha256_hex
unique_keys:
    - attributes: ["machine", "path"]

object_type: FileTags
attributes:
    - name: tags
      data_type: list:string
    - name: tags
      data_type: list:string
unique_keys:
    - attributes: ["machine", "path"]

object_type: WebsiteSnapshot
attributes:
    - name: website
      data_type: ref:Website@url
    - name: file
      data_type: ref:File@sha256_hex
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
