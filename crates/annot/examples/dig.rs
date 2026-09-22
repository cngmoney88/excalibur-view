use annot::measure::Measure;
use annot::place::read_page;
use pdf::Document;
fn main(){
    let doc = Document::open(std::env::args().nth(1).unwrap()).unwrap();
    for index in 0..doc.page_count() {
        for (_, m) in read_page(&doc, index) {
            let it = m.dict.get("IT").and_then(|o| o.as_name()).map(|n| n.as_str().to_string()).unwrap_or_default();
            if !it.contains("Polygon") { continue }
            let pts = m.points();
            let n = pts.len();
            let mut twice = 0.0;
            for i in 0..n { let a=pts[i]; let b=pts[(i+1)%n]; twice += a[0]*b[1]-b[0]*a[1]; }
            let sq = (twice*0.5).abs();
            let measure = m.dict.get("Measure").and_then(|o| o.as_dict()).and_then(Measure::read);
            if let Some(me) = &measure {
                let per = me.per_point();
                println!("{:<10} stored={:<12} exact={:>12.6} sf  mine={:<12}",
                    m.subject(), m.contents(), sq*per*per, me.area(sq));
                let a = m.dict.get("Measure").and_then(|o| o.as_dict()).and_then(|d| d.get("A")).cloned();
                println!("            A format = {:?}", a);
            }
        }
    }
}
