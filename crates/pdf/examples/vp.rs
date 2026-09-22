fn main(){
    for path in std::env::args().skip(1) {
        let doc = pdf::Document::open(&path).unwrap();
        println!("\n{}", path.rsplit('/').next().unwrap());
        for i in 0..doc.page_count().min(6) {
            let Some(page) = doc.page(i) else { continue };
            let number = doc.at(&page, "revit_metadata_view_sheet_number").as_text();
            let name = doc.at(&page, "revit_metadata_view_name").as_text();
            let kind = doc.at(&page, "revit_metadata_view_type").as_text();
            println!("  p{}: number={number:?} name={name:?} type={kind:?}", i+1);
        }
        let labels = doc.at(&doc.catalog(), "PageLabels");
        if let Some(d) = labels.as_dict() {
            let nums = doc.at(d, "Nums");
            let a = nums.as_array().map(|a| a.len()).unwrap_or(0);
            println!("  PageLabels: Nums has {a} entries");
            if let Some(items) = nums.as_array() {
                for pair in items.chunks(2).take(4) {
                    let at = pair[0].as_i64();
                    let spec = doc.follow(&pair[1]);
                    let d = spec.as_dict();
                    println!("    from page {at:?}: {:?}", d.map(|d| d.iter().map(|(k,v)| format!("{}={:?}", k.as_str(), v)).collect::<Vec<_>>()));
                }
            }
        }
    }
}
