package api

import (
	"net/http"

	"github.com/gin-gonic/gin"
)

func registerRoutes(r gin.IRouter) {
	r.GET("/ping", func(c *gin.Context) {
		c.JSON(http.StatusOK, gin.H{
			"message": "pong",
		})
	})
}
