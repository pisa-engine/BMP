
<p align="center">
    <img width="100px" src="../img/logo.jpg" />
    <img width="100px" src="../img/plus.png" />
    <img width="100px" src="../img/python.png" />
    <h1 align="center">BMP</h1>
</p>

## Install

From PyPi:

```
pip install bmp
```

From Source:

```
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
...
indexer.finish()
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
