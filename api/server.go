package api

import (
	"context"

	"github.com/gin-gonic/gin"
)

type APIConfig struct {
	APIEndpoint string
}

func Serve(ctx context.Context, cfg APIConfig) error {
	r := gin.Default()
	registerRoutes(r)
	return r.Run(cfg.APIEndpoint)
}
