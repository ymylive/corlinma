# corlinman-tagmemo

Energy-Perception-Action (EPA) projection + Residual Pyramid features over
chunk embedding vectors.

Pure NumPy / scikit-learn. No gRPC, no PyO3, no cross-crate wiring — that
lives in B3-BE5+.

## Public API

```python
from corlinman_tagmemo import (
    EpaBasis, EpaProjection, DominantAxis, fit_basis, project,
    PyramidResult, PyramidLevel, PyramidFeatures, build_pyramid,
    dynamic_boost,
)
```
