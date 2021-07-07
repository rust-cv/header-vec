# header-vec

Allows one to store a header struct and a vector all inline in the same memory on the heap and share weak versions for minimizing random lookups in data structures

If you use this without creating a weak ptr, it is safe. It is unsafe to create a weak pointer because you now have aliasing.
