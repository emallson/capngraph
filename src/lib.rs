extern crate capnp;
extern crate petgraph;

use petgraph::visit::EdgeRef;
use petgraph::{Direction, Graph};

pub mod graph_capnp {
    include!(concat!(env!("OUT_DIR"), "/graph_capnp.rs"));
}
use capnp::serialize_packed;
use graph_capnp::{edge, graph_header};

use std::fs::File;
use std::io::{BufReader, BufWriter, Error};

pub type NodeId = u32;

pub fn load_edges(p: &str) -> capnp::Result<Vec<(NodeId, NodeId, f32)>> {
    let f = File::open(p)?;
    let mut reader = BufReader::new(f);
    let opts = ::capnp::message::ReaderOptions::new();
    let msg = serialize_packed::read_message(&mut reader, opts)?;
    let header_root: graph_header::Reader = msg.get_root().unwrap();
    let mut edges = Vec::with_capacity(header_root.get_num_edges() as usize);

    while let Ok(msg) = serialize_packed::read_message(&mut reader, opts) {
        let edge_root: edge::Reader = msg.get_root().unwrap();

        let mut edgelist = match edge_root.get_to().which().unwrap() {
            edge::to::Node(to) => match edge_root.get_weight().which().unwrap() {
                edge::weight::Value(weight) => vec![(edge_root.get_from(), to, weight)],
                _ => panic!("Single node given with List of Weights"),
            },
            edge::to::List(Ok(to_list)) => match edge_root.get_weight().which().unwrap() {
                edge::weight::List(Ok(w_list)) => to_list
                    .iter()
                    .zip(w_list.iter())
                    .map(|(to, w)| (edge_root.get_from(), to, w))
                    .collect::<Vec<(u32, u32, f32)>>(),
                _ => panic!("Single weight given with List of Nodes"),
            },
            _ => panic!("Unable to read edge"),
        };

        edges.append(&mut edgelist);
    }

    Ok(edges)
}

pub fn load_graph(p: &str) -> capnp::Result<Graph<(), f32>> {
    load_edges(p).map(Graph::from_edges)
}

pub fn write_graph(p: &str, tag: &str, g: &Graph<(), f32>) -> Result<(), Error> {
    let f = File::create(p)?;
    let mut writer = BufWriter::new(f);

    // write header
    {
        let mut message = ::capnp::message::Builder::new_default();
        {
            let mut header = message.init_root::<graph_header::Builder>();
            header.set_tag(tag);
            header.set_num_nodes(g.node_count() as u32);
            header.set_num_edges(g.edge_count() as u64);
        }
        serialize_packed::write_message(&mut writer, &message)?;
    }

    for node in g.node_indices() {
        let edges = g.edges_directed(node, Direction::Outgoing);
        let num_edges = edges.clone().count() as u32;
        let mut message = ::capnp::message::Builder::new_default();
        {
            let mut em = message.init_root::<edge::Builder>();
            em.set_from(node.index() as u32);
            {
                let mut to = em.reborrow().get_to().init_list(num_edges);
                for (i, edgeref) in edges.clone().enumerate() {
                    to.set(i as u32, edgeref.target().index() as u32);
                }
            }
            {
                let mut weight = em.reborrow().get_weight().init_list(num_edges);
                for (i, edgeref) in edges.clone().enumerate() {
                    weight.set(i as u32, *edgeref.weight());
                }
            }
        }
        serialize_packed::write_message(&mut writer, &message)?;
    }
    Ok(())
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn read_petgraph() {
        let g = load_graph("data/bin/ca-GrQc.bin").unwrap();
        assert!(g.node_count() == 5242);
        assert!(g.edge_count() == 28980);
    }

    #[test]
    fn read_petgraph_grouped() {
        let g = load_graph("data/bin/ca-GrQc_grouped.bin").unwrap();
        assert!(g.node_count() == 5242);
        assert!(g.edge_count() == 28980);
    }
}
