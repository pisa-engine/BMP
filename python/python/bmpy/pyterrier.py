import bmpy

try:
    import pyterrier as pt
    import pandas as pd

    class BmpRetriever(pt.Transformer):
        def __init__(
            self,
            path: str,
            *,
            num_results: int = 1000,
            alpha: float = 1.,
            beta: float = 1.
        ):
            self.searcher = bmpy.Searcher(path)
            self.num_results = num_results
            self.alpha = alpha
            self.beta = beta
            self.batch_size = 32 # this should come from the index

        def transform(self, inp):
            assert 'query_toks' in inp.columns
            assert 'qid' in inp.columns
            result = {
                'qid': [],
                'docno': [],
                'score': [],
                'rank': [],
            }
            for qid, query_toks in zip(inp['qid'], inp['query_toks']):
                docnos, scores = self.searcher.search(
                    query_toks,
                    k=self.num_results,
                    alpha=self.alpha,
                    beta=self.beta,
                    batch_size=self.batch_size)
                result['qid'].extend([qid] * len(docnos))
                result['docno'].extend(docnos)
                result['score'].extend(scores)
                result['rank'].extend(range(len(docnos)))
            return pd.DataFrame(result)

except ImportError:
    def BmpRetriever(*args, **kwargs):
        raise RuntimeError('python-terrier required to use BmpRetriever')
