
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
import string
import random
indexer = Indexer('/path/to/index', bsize=32, compress_range=False)
terms = [(c, []) for c in string.ascii_letters]
for doc in range(10_000):
    dvec = []
    for idx in range(random.randrange(1, 10)):
        tf = random.randrange(1, 1000)
        tok = random.randrange(len(terms))
        dvec.append((tok, tf))
        terms[tok][1].append((doc, tf))
    indexer.add_document(f'doc{doc}', dvec)
for term, postings in terms:
    indexer.add_term(term, postings)
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
