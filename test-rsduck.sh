#!/bin/bash

# Default number of times to run
NUM_RUNS=4

# Parse command line arguments
while getopts "n:" opt; do
  case $opt in
    n)
      NUM_RUNS=$OPTARG
      ;;
    \?)
      echo "Usage: $0 [-n number_of_runs]"
      exit 1
      ;;
  esac
done

# Run curl commands in the background
for i in $(seq 1 $NUM_RUNS); do
  curl -X 'GET' \
    'http://localhost:3001/query?sql=pragma%20tpch%2820%29' \
    -H 'accept: application/json' &
done

wait
