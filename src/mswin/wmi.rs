extern crate wmi;

pub fn wmi_query() -> Result<i32, Box<std::error::Error>> {    
    use wmi::{COMLibrary, WMIConnection};
    let com_con = COMLibrary::new().unwrap();
    let wmi_con = WMIConnection::new(com_con.into()).unwrap();

    use std::collections::HashMap;
    use wmi::Variant;
    let results: Vec<HashMap<String, Variant>> = wmi_con.raw_query("SELECT * FROM Win32_OperatingSystem").unwrap();

    for os in results {
        println!("{:#?}", os);
    }
    Ok(0)
}    
