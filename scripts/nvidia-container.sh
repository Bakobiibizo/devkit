#!/bin/bash
 
TTY_FLAGS=()
test -t 0 && TTY_FLAGS=(-it)
 
docker run --rm "${TTY_FLAGS[@]}" --runtime=nvidia --gpus=all --ulimit memlock=-1 --ulimit stack=67108864 nvcr.io/nvidia/pytorch:25.09-py3 /bin/bash
