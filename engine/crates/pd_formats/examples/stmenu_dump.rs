use pd_formats::alr::Alr;
fn main() {
    let a = Alr::load(std::path::Path::new(&std::env::args().nth(1).unwrap())).unwrap();
    for (mi, m) in a.models.iter().enumerate() {
        for (i, x) in m.materials.iter().enumerate() {
            if x.scroll.iter().any(|&v| v != 0) || std::env::var("ALL").is_ok() {
                println!("model {mi} mat {i}: fmt {} tex {} n {} t2 {} t3 {} flags {:#x} scroll {:?}", x.vertbuf_format, x.texture_idx, x.normal_idx, x.texture_idx2, x.texture_idx3, x.flags, x.scroll);
            }
        }
    }
}
