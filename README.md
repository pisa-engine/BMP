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

### Usage

#### Data
The CIFF files and the queries required by BMP to generate an index and perform search operations can be found in the so called [CIFF-Hub](https://github.com/pisa-engine/ciff-hub/tree/main).

**One requirement for BMP to work correctly is that the impact scores of the CIFF files have to be quantized to 8 bits. This is not always done and for this reason is highly recommended to use the CIFF files from the Hub**

#### Index
```
./target/release/ciff2bmp -b 8 -c ./bp-msmarco-passage-unicoil-quantized.ciff -o bp-msmarco-passage-unicoil-quantized.bmp --compress-range
```
#### Search
```
./target/release/search --index bp-msmarco-passage-unicoil-quantized.bmp --k 1000 --queries dev.pisa > bp-msmarco-passage-unicoil-quantized.dev.trec
```
#### Evaluate
```
trec_eval -M 10 -m recip_rank qrels.msmarco-passage.dev-subset.txt bp-msmarco-passage-unicoil-quantized.dev.trec
```

## Python Bindings

<p align="center">
    <img width="100px" src="./img/logo.jpg" />
    <img width="100px" src="./img/plus.png" />
    <img width="100px" src="./img/python.png" />
    <h1 align="center">BMP</h1>
</p>

### Install

Form PyPi:

```sh
pip install bmp
```

or 

```sh
pip install git+https://github.com/pisa-engine/BMP.git#subdirectory=python
```

From source (in the 'python' directory, i.e `cd python`):

```
pip install maturin
maturin build -r
pip install target/wheels/*.whl
```

### Usage
#### Index
```python
from bmp import ciff2bmp

ciff2bmp(ciff_file="/path/to/ciff", output="/path/to/index", bsize=32, compress_range=False)
```
#### Search

```python
from bmp import search, Searcher

# batch operation
results = search(index="/path/to/index", queries="/path/to/queries", k=10, alpha=1.0, beta=1.0)
# -> str (TREC run file)

# query-by-query operation
searcher = Searcher("/path/to/index") # loads index into memory once
searcher.search({'tok1': 5.3, 'tok2': 1.1}, k=10, alpha=1.0, beta=1.0)
# -> Tuple[List[str], List[float]] (doc IDs, scores) for this query
```
