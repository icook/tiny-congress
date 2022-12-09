package main

import (
	"encoding"
	"encoding/json"

	"github.com/pkg/errors"
)

type ListItem interface {
	encoding.TextMarshaler
	encoding.TextUnmarshaler
	// String() string
}

// Serves as a reference type implementation. All write methods must return a
// new copy for easy tracing of state transitions.
// PersistenceLayer needs only the `encoding.Text(Un)Marshaler` method to store
type List struct {
	data      []ListItem
	ItemMaker func(text string) (ListItem, error)
}

func (l List) newList(newData []ListItem) List {
	return List{
		data:      newData,
		ItemMaker: l.ItemMaker,
	}
}

func (l List) Insert(index int, value ListItem) (List, error) {
	if len(l.data) == index { // nil or empty slice or after last element
		return l.newList(append(l.data, value)), nil
	}
	newData := append(l.data[:index+1], l.data[index:]...) // index < len(a)
	newData[index] = value
	return l.newList(newData), nil
}

func (l List) Append(value ListItem) (List, error) {
	return l.newList(append(l.data, value)), nil
}

func (l List) Prepend(value ListItem) (List, error) {
	return l.newList(append([]ListItem{value}, l.data...)), nil
}

func (l List) MarshalText() ([]byte, error) {
	var buf = make([]string, len(l.data))
	for i := range l.data {
		encoded, err := l.data[i].MarshalText()
		if err != nil {
			return nil, errors.WithStack(err)
		}
		buf[i] = string(encoded)
	}
	return json.Marshal(buf)
}

func (l *List) UnmarshalText(text []byte) error {
	var inputs []string
	err := json.Unmarshal(text, &inputs)
	var newData = make([]ListItem, len(inputs))
	for i := range inputs {
		decoded, err := l.ItemMaker(inputs[i])
		if err != nil {
			return errors.WithStack(err)
		}
	}
	return err
}
