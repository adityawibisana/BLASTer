//! Debug helper: print the bases and qualities of a region of one read.
//! Usage: cargo run --example dump_tail -- <zip> <sample> <from> <to>

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let imported = coi_blast_lib::importer::import_zip(args[1].as_ref(), &Default::default()).unwrap();
    let read = imported.reads.iter().find(|r| r.id == args[2]).expect("sample");
    let from: usize = args[3].parse().unwrap();
    let to: usize = args[4].parse::<usize>().unwrap().min(read.raw_length);
    println!("{}", &read.bases[from..to]);
    println!("{:?}", &read.quals[from..to]);
}
