package main

import (
	"fmt"
	"os"

	"hyperlang.dev/hyperc/internal/formatter"
)

func main() {
	if len(os.Args) < 2 {
		usage()
		os.Exit(1)
	}

	var path string
	checkOnly := false
	toStdout := false
	for _, a := range os.Args[1:] {
		switch a {
		case "--check":
			checkOnly = true
		case "--stdout":
			toStdout = true
		case "-h", "--help":
			usage()
			return
		default:
			path = a
		}
	}
	if path == "" {
		usage()
		os.Exit(1)
	}

	src, err := os.ReadFile(path)
	if err != nil {
		fmt.Fprintln(os.Stderr, "hyperfmt: nie można odczytać", path, "-", err)
		os.Exit(1)
	}

	formatted, err := formatter.Format(string(src))
	if err != nil {
		fmt.Fprintln(os.Stderr, "hyperfmt:", err)
		os.Exit(1)
	}

	if toStdout {
		fmt.Print(formatted)
		return
	}

	if checkOnly {
		if formatted != string(src) {
			fmt.Fprintf(os.Stderr, "%s: niesformatowany\n", path)
			os.Exit(1)
		}
		fmt.Println(path + ": OK")
		return
	}

	if formatted == string(src) {
		return
	}
	if err := os.WriteFile(path, []byte(formatted), 0644); err != nil {
		fmt.Fprintln(os.Stderr, "hyperfmt: nie można zapisać:", err)
		os.Exit(1)
	}
	fmt.Println(path + ": sformatowano")
}

func usage() {
	fmt.Fprintln(os.Stderr, `hyperfmt - formatter Hyper Lang

Użycie:
  hyperfmt plik.hyper [--check] [--stdout]`)
}
