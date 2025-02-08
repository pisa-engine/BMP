import numpy as np
from pathlib import Path
import pyterrier as pt
import pyterrier_alpha as pta
from bmp import Indexer, Searcher

class BmpIndex(pta.Artifact, pt.Indexer):
    ARTIFACT_TYPE = 'sparse_index'
    ARTIFACT_FORMAT = 'bmp'
    def __init__(self, path: str, *, verbose: bool = True):
        super().__init__(path)
        self.verbose = verbose
        self._searcher = None
    def built(self) -> bool:
        return Path(self.path).exists()
    def indexer(self, bsize=32, compress_range=False, scale_float=100.):
        return BmpIndexer(self, bsize=bsize, compress_range=compress_range, scale_float=scale_float)
    def index(self, inp):
        return self.indexer().index(inp)
    def retriever(self):
        return BmpRetriever(self)
    def transform(self, inp):
        return self.retriever()(inp)
    def load_into_memory(self):
        if self._searcher is None:
            self._searcher = Searcher(str(self.path/'index.bmp'))
        return self._searcher
    def __enter__(self):
        return self
    def __exit__(self, exc_type, exc_val, exc_tb):
        self.close()
    def close(self):
        self._searcher = None

class BmpIndexer(pt.Indexer):
    def __init__(self, bmp_index, bsize=32, compress_range=False, scale_float=100.):
        self.bmp_index = bmp_index
        self.bsize = bsize
        self.compress_range = compress_range
        self.scale_float = scale_float
    def index(self, inp):
        assert not self.bmp_index.built()
        with pta.ArtifactBuilder(self.bmp_index) as builder:
            indexer = Indexer(str(self.bmp_index.path/'index.bmp'), bsize=self.bsize, compress_range=self.compress_range)
            count = 0
            for doc in inp:
                vector = doc['toks']
                if len(vector) > 0 and isinstance(next(iter(vector.values())), float):
                    vector = {k: int(v * self.scale_float) for k, v in vector.items()}
                indexer.add_document(doc['docno'], vector)
                count += 1
            indexer.finish()
            builder.metadata['bsize'] = self.bsize
            builder.metadata['compress_range'] = self.compress_range
            builder.metadata['scale_float'] = self.scale_float
            builder.metadata['num_docs'] = count
        return self

class BmpRetriever(pt.Indexer):
    def __init__(self, bmp_index, *, num_results=1000, alpha=1.0, beta=1.0):
        self.bmp_index = bmp_index
        self.num_results = num_results
        self.alpha = alpha
        self.beta = beta
    def transform(self, inp):
        pta.validate.query_frame(inp, extra_columns=['query_toks'])
        searcher = self.bmp_index.load_into_memory()
        res = pta.DataFrameBuilder(['docno', 'score', 'rank'])
        for toks in inp['query_toks']:
            docnos, scores = searcher.search(toks, k=self.num_results, alpha=self.alpha, beta=self.beta)
            res.extend({
                'docno': docnos,
                'score': scores,
                'rank': np.arange(len(scores))
            })
        return res.to_df(inp)
    def fuse_rank_cutoff(self, k):
        if self.num_results > k:
            return BmpRetriever(self.bmp_index, num_results=k, alpha=self.apha, beta=self.beta)
