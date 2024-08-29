fn main() {
    let f = std::env::args().nth(1).expect("no arg passed");
    let slp = std::fs::read(&f).unwrap();

    let t = std::time::Instant::now();

    let mut compressor = slp_compress::Compressor::new(4).unwrap();
    let ret = slp_compress::compress(&mut compressor, &slp).unwrap();

    let d = std::time::Instant::now();
    println!("compress in {}ms", (d - t).as_secs_f64() * 1000.0);

    let mut decompressor = slp_compress::Decompressor::new().unwrap();
    let slp_round_trip = slp_compress::decompress(&mut decompressor, &ret).unwrap();

    println!("decompress in {}ms", d.elapsed().as_secs_f64() * 1000.0);

    let mut out = f.clone();
    out.push_str(".slpz");
    std::fs::write(&out, &ret).unwrap();

    // out.push_str(".slp");
    // std::fs::write(&out, &slp_round_trip).unwrap();

    assert!(&slp_round_trip == &slp);
}
