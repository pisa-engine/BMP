
<p align="center">
    <img width="100px" src="../img/logo.jpg" />
    <img width="100px" src="../img/plus.png" />
    <img width="100px" src="../img/python.png" />
    <h1 align="center">BMPy</h1>
</p>

## Install
```
pip install maturin
maturin build -r
pip install target/wheels/*.whl
```

## Usage
### Index
```python
from bmpy import ciff2bmp
ciff2bmp(ciff_file="/path/to/ciff", output="/path/to/index", bsize=32, compress_range=False)
```
