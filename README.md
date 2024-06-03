<p align="center">
    <img width="200px" src="img/logo.jpg" />
    <h1 align="center">BMP</h1>
</p>

# Faster Learned Sparse Retrieval with Block-Max Pruning

This repository contains the source code used for the experiments presented in the paper "Faster Learned Sparse Retrieval with Block-Max Pruning" by Antonio Mallia, Torsten Suel and Nicola Tonellotto, published at SIGIR, 2024 - [PDF](https://arxiv.org/pdf/2405.01117). 

Please cite the following paper if you use this code, or a modified version of it:

```bibtex
@inproceedings{BMP,
  author = {Antonio Mallia and Torsten Suel and Nicola Tonellotto},
  title = {Faster Learned Sparse Retrieval with Block-Max Pruning},
  booktitle = {The 47th International ACM SIGIR Conference on Research and Development in Information Retrieval ({SIGIR})},
  publisher = {ACM},
  year = {2024}
}
```

# Usage

## Data
The CIFF files and the queries required by BMP to generate an index and perform search operations can be found in the so called [CIFF-Hub](https://github.com/pisa-engine/ciff-hub/tree/main).

## Index
```
./target/release/ciff2bmp -b 8 -c ./bp-msmarco-passage-unicoil-quantized.ciff -o bp-msmarco-passage-unicoil-quantized.bmp --compress-range
```
## Search
```
./target/release/search --bsize 8 --index bp-msmarco-passage-unicoil-quantized.bmp --k 1000 --queries dev.pisa > bp-msmarco-passage-unicoil-quantized.dev.trec
```
## Evaluate
```
trec_eval -M 10 -m recip_rank qrels.msmarco-passage.dev-subset.txt bp-msmarco-passage-unicoil-quantized.dev.trec
```