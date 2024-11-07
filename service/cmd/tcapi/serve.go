package main

import (
	"context"

	"github.com/icook/tiny-congress/api"
	"github.com/spf13/cobra"
)

func init() {
	rootCmd.AddCommand(serveCmd)
}

var serveCmd = &cobra.Command{
	Use:   "serve",
	Short: "Serve the API web service",
	RunE: func(cmd *cobra.Command, args []string) error {
		return api.Serve(context.Background(), api.APIConfig{
			APIEndpoint: "localhost:8080",
		})
	},
}
