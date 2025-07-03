# BMP - Block-Max Pruning

Faster Learned Sparse Retrieval with Block-Max Pruning

## Installation

From PyPI:

```bash
pip install bmp
```

From source:

```bash
pip install maturin
maturin build -r
pip install target/wheels/*.whl
```

## Usage

### Index from CIFF

```python
from bmp import ciff2bmp
ciff2bmp(ciff_file="/path/to/ciff", output="/path/to/index", bsize=32, compress_range=False)
```

### Index with Python

```python
from bmp import Indexer
indexer = Indexer('/path/to/index', bsize=32, compress_range=False)
indexer.add_document('doc1', {'a': 1, 'b': 5, 'c': 8}) # docid, vector
indexer.add_document('doc2', {'a': 2, 'c': 1, 'd': 8, 'f': 2})
# ... add more documents
indexer.finish()
```

### Search

```python
from bmp import search, Searcher

# Batch operation
results = search(index="/path/to/index", queries="/path/to/queries", k=10, alpha=1.0, beta=1.0)
# Returns: str (TREC run file)

# Query-by-query operation
searcher = Searcher("/path/to/index") # loads index into memory once
searcher.search({'tok1': 5.3, 'tok2': 1.1}, k=10, alpha=1.0, beta=1.0)
# Returns: Tuple[List[str], List[float]] (doc IDs, scores) for this query
```

## Citation

If you use this code, please cite:

```bibtex
@inproceedings{BMP,
  author = {Antonio Mallia and Torsten Suel and Nicola Tonellotto},
  title = {Faster Learned Sparse Retrieval with Block-Max Pruning},
  booktitle = {The 47th International ACM SIGIR Conference on Research and Development in Information Retrieval ({SIGIR})},
  publisher = {ACM},
  year = {2024}
}
```

## License

Apache-2.0
