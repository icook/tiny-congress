package db

import (
	"net/url"
	"testing"

	"github.com/icook/tiny-congress/storage/mem"
	"github.com/stretchr/testify/assert"
)

func makeURL(t *testing.T, s string) url.URL {
	t.Helper()
	u, err := url.Parse(s)
	assert.NoError(t, err)
	return u
}

func TestPersistenceLayer(t *testing.T) {
	t.Run("normal", func(t *testing.T) {
		store := mem.Store()
		p := PersistenceLayer{
			store: store,
			obj: nil,
		}

		_, err := p.FetchObject(FetchObjectRequest{
			URI: makeURL(t, "file://sha/?sha256_sum=42c79fd316123b7acfc99d7e0c3bdbe0d0df144cd7b48fb11e2ba5c8699dcdb0"),
			// URI: makeURL("/file/sha/42c79fd316123b7acfc99d7e0c3bdbe0d0df144cd7b48fb11e2ba5c8699dcdb0"),
		})
		assert.NoError(t, err)
	})
}
