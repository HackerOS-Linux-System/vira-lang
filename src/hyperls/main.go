package main

import (
	"fmt"
	"os"

	"hyperlang.dev/hyperc/internal/lsp"
)

func main() {
	if err := lsp.Run(os.Stdin, os.Stdout); err != nil {
		fmt.Fprintln(os.Stderr, "hyperls:", err)
		os.Exit(1)
	}
}
