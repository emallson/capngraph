extern crate capnp;
pub mod graph_capnp {
    include!(concat!(env!("OUT_DIR"), "/graph_capnp.rs"));
}
use capnp::serialize_packed;
use graph_capnp::{edge, graph_header};
extern crate docopt;
use docopt::Docopt;
extern crate rustc_serialize;
use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter, Read};
use std::path::Path;
use std::str;

const USAGE: &'static str = "
Convert the input edge-list to a packed binary file readable by Cap'n
Proto.

The input edge-list should have the number of nodes and edges on the
first line, followed by #edges lines of <from> <to> <weight> triplets.

Usage:
  convert <source> <dest> [--tag=<tag>] [--grouped]
  convert (-h | --help)

Options:
  -h --help          Show this screen.
  --tag=<tag>        The tag (name) of the graph. Defaults to the basename of the source.
  --grouped          The <source> is grouped by source node. Enable only if true, produces a smaller output.
";

#[derive(RustcDecodable)]
struct Args {
    arg_source: String,
    arg_dest: String,
    flag_tag: Option<String>,
    flag_grouped: bool,
}

struct EdgeIter<R: Read> {
    input: BufReader<R>,
}

type Edge = (u32, u32, f32);

impl<R: Read> EdgeIter<R> {
    fn read_header<S: BufRead>(input: &mut S) -> Result<(u32, u64), String> {
        let mut first_line = String::new();
        input.read_line(&mut first_line).unwrap();
        let first_row = first_line.split_whitespace().collect::<Vec<&str>>();
        if first_row.len() != 2 {
            return Err(format!(
                "Header inappropriate length: {}. Expected 2.",
                first_row.len()
            ));
        }
        assert!(first_row.len() == 2);

        Ok((
            first_row[0].parse::<u32>().unwrap(),
            first_row[1].parse::<u64>().unwrap(),
        ))
    }

    /// Construct an `EdgeIter` instance and read the header of the
    /// data, returning both.
    pub fn init(input: R) -> Result<(Self, (u32, u64)), String> {
        let mut input = BufReader::new(input);
        let header = Self::read_header(&mut input)?;
        let iter = EdgeIter { input: input };

        Ok((iter, header))
    }
}

impl<R: Read> Iterator for EdgeIter<R> {
    type Item = Edge;

    fn next(&mut self) -> Option<Self::Item> {
        let mut line = String::new();
        match self.input.read_line(&mut line) {
            Ok(_len) => {
                let row = line.split_whitespace().collect::<Vec<&str>>();

                if row.len() == 3 {
                    // These will panic if they fail to parse -- this is
                    // intentional. Would rather have the conversion stop
                    // than end up with an incomplete (and incorrect)
                    // dataset.
                    Some((
                        row[0].parse::<u32>().unwrap(),
                        row[1].parse::<u32>().unwrap(),
                        row[2].parse::<f32>().unwrap(),
                    ))
                } else {
                    None
                }
            }
            _ => None,
        }
    }
}

type HeapBuilder = ::capnp::message::Builder<::capnp::message::HeapAllocator>;
fn build_edgelist(from: u32, edges: &Vec<(u32, f32)>) -> HeapBuilder {
    let mut message = ::capnp::message::Builder::new_default();
    {
        let mut em = message.init_root::<edge::Builder>();
        em.set_from(from);
        {
            let mut to = em.reborrow().get_to().init_list(edges.len() as u32);
            for (i, &(t, _)) in edges.iter().enumerate() {
                to.set(i as u32, t);
            }
        }
        {
            let mut weight = em.reborrow().get_weight().init_list(edges.len() as u32);
            for (i, &(_, w)) in edges.iter().enumerate() {
                weight.set(i as u32, w);
            }
        }
    }

    message
}

fn build_edge(from: u32, to: u32, weight: f32) -> HeapBuilder {
    let mut message = ::capnp::message::Builder::new_default();
    {
        let mut edge = message.init_root::<edge::Builder>();
        edge.set_from(from);
        edge.reborrow().get_to().set_node(to);
        edge.reborrow().get_weight().set_value(weight);
    }
    message
}

fn main() {
    let args: Args = Docopt::new(USAGE)
        .and_then(|d| d.decode())
        .unwrap_or_else(|e| e.exit());

    let (iter, (num_nodes, num_edges)) =
        EdgeIter::init(File::open(&args.arg_source).unwrap()).unwrap();
    let f = File::create(&args.arg_dest).unwrap();
    let mut writer = BufWriter::new(f);
    // building the capnp message
    let mut message = ::capnp::message::Builder::new_default();
    {
        let mut header = message.init_root::<graph_header::Builder>();
        if let Some(tag) = args.flag_tag {
            header.set_tag(tag.as_str());
        } else {
            header.set_tag(
                Path::new(&args.arg_source)
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap(),
            )
        }

        header.set_num_nodes(num_nodes);
        header.set_num_edges(num_edges);
    }
    serialize_packed::write_message(&mut writer, &message).unwrap();

    let mut num_read_edges = 0;

    if args.flag_grouped {
        let mut prev_from = None;
        let mut edges = vec![];
        for (from, to, weight) in iter {
            num_read_edges += 1;
            if let Some(prev) = prev_from {
                // we have an existing edgelist
                if prev == from {
                    edges.push((to, weight)); // matching source, add edge to list
                } else {
                    // new source, write out previous edge list
                    let message = build_edgelist(prev, &edges);
                    serialize_packed::write_message(&mut writer, &message).unwrap();
                    // clear previous edgelist
                    edges.clear();
                    // add current edge
                    edges.push((to, weight));
                }
            } else if let None = prev_from {
                // no existing edgelist, add current edge
                edges.push((to, weight));
            }
            prev_from = Some(from);
        }
        // write out the last edgelist
        if let Some(prev) = prev_from {
            let message = build_edgelist(prev, &edges);
            serialize_packed::write_message(&mut writer, &message).unwrap();
        }
    } else {
        for (from, to, weight) in iter {
            num_read_edges += 1;
            let message = build_edge(from, to, weight);
            serialize_packed::write_message(&mut writer, &message).unwrap();
        }
    }
    if num_edges != num_read_edges {
        panic!(
            "Expected {} edges, read {} edges",
            num_edges, num_read_edges
        );
    }
}
