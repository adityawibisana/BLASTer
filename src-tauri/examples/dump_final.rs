//! Debug helper: print final sequences, their table 5 translation, and which
//! sample contains a given motif.
//! Usage: cargo run --example dump_final -- <zip> <motif> [sample...]

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let imported = coi_blast_lib::importer::import_zip(args[1].as_ref(), &Default::default()).unwrap();
    for r in &imported.reads {
        if let Some(pos) = r.derived.final_seq.find(&args[2]) {
            println!("motif in {} final seq at {} (final len {})", r.id, pos, r.derived.final_len);
        }
        if let Some(pos) = r.bases.find(&args[2]) {
            println!("motif in {} raw read at {}", r.id, pos);
        }
    }
    for id in &args[3..] {
        let r = imported.reads.iter().find(|r| &r.id == id).unwrap();
        let s = r.derived.final_seq.as_bytes();
        println!("== {id} len {}\n{}", s.len(), r.derived.final_seq);
        for f in 0..3 {
            println!("frame {f}: {}", coi_blast_lib::seq::translate_table5(s, f));
        }
    }
}
