# clang_mcp.py Quickstart

## Install dependency

```bash
sudo apt-get update
sudo apt-get install -y python3-clang
```

## Prepare compile database

```bash
CC=clang CXX=clang++ cmake -S . -B build
```

## Run

```bash
python3 clang_mcp.py doctor
python3 clang_mcp.py --build-dir build --file sample.cpp list-functions
python3 clang_mcp.py --build-dir build --file sample.cpp describe-function --name add
```
