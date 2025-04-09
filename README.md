# header-vec

Allows one to store a header struct and a vector all inline in the same memory on the heap and share weak versions for minimizing random lookups in data structures

If you use this without creating a weak ptr, it is safe. It is unsafe to create a weak pointer because you now have aliasing.

## Features

* `std`  
  Enables API's that requires stdlib features. Provides more compatibility to `Vec`.
  This feature is enabled by default.
* `atomic_append`  
  Enables the `atomic_push` API's that allows extending a `HeaderVec` with interior
  mutability from a single thread by a immutable handle.
  This feature is enabled by default.
