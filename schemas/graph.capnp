@0xaed9fb729c576ca5;

struct WeightedDirectedGraph {
    tag @0 : Text;
    numNodes @1 : UInt32;

    struct Edge {
        from @0 : UInt32;
        to @1 : UInt32;
        weight @2 : Float32;
    }

    edges @2 : List(Edge);
}
