use serde_json::json;
use time::PrimitiveDateTime;
use time_macros::datetime;

pub fn prepare_data() -> Vec<Work> {
    vec![
        create_work_001(),
        create_work_002(),
        create_work_003(),
        create_work_004(),
        create_work_005(),
    ]
}

fn create_work_001() -> Work {
    let mut object_001 = Object::new(
        "object_001_001",
        "Zip",
        None,
        3_000_000,
        json!({"md5":"00cf96e7b9b95dfdd83f44ba0683523d", "sha1":"e7c8d333ae4970119e74ef023e3e55a6f7234ba0", "sha256":"1042cfc188710bad4809ec4f3b79f94865a223482327b4d4b7a784e65d1b7926"}),
        json!({ "ok": { "symbols" : ["TOP_SECRET"], "object_metadata": {"array":[{"key": "From", "value":"A"},{"key": "To", "value":"B"},{"key": "Subject", "value":"Ctx"}]} }}),
    );
    let object_002 = Object::new(
        "object_001_002",
        "PE",
        None,
        2_000_000,
        json!({"md5":"e8550d9254a0881407ca764f63dde03c", "sha1":"9a670345cdc93ee6e09af49d41a4849a6ec75b50", "sha256":"831c1c91ab689fb668f2d8705523f46902e7b9e9beac0d69b9b3de4f60c3028d"}),
        json!({ "ok": { "symbols" : ["DLL"], "object_metadata": {"test_escaping":"ascii"} }}),
    );
    let object_003 = Object::new(
        "object_001_003",
        "Lnk",
        None,
        5_000,
        json!({"md5":"65cab523e0f9880cfea5eecf521d9782", "sha1":"68c4f983ffed3a3492b1e429fc14aa92978f38a2", "sha256":"4d55c8e23e68f5335ba5bff0a415c288da0103a30bbd833a41ca46bd026d41c3"}),
        json!({ "ok": { "symbols" : ["A","B"], "object_metadata": {"test_escaping":"\u{1F332}\u{1F333}\u{1F334}"} }}),
    );
    let mut object_004 = Object::new(
        "object_001_004",
        "HTML",
        None,
        40_000,
        json!({"md5":"318d4318256e419e05050811af06daa5", "sha1":"40b28fad3d1602ee1bf08bde9c87f1b2e6ffe87f", "sha256":"fe33697c08603e798fa5c42c0b26878e60a8d203a0320c1472c4ad0d49245210"}),
        json!({ "ok": { "symbols" : [], "object_metadata": {"int1":2, "int2":3, "bool": true, "string": "string1", "array": [2,3]} }}),
    );
    let object_005 = Object::new(
        "object_001_005",
        "Text",
        None,
        3_000_000,
        json!({"md5":"a8a2d199b634527e7f7fa0f3ddf580b4", "sha1":"07b6e5a00d6a22cc40d60cf47794841db0033814", "sha256":"2385ded576c39f9e708df69fa25aeaa58cc87eb176991a1b1899c1113d7c0741"}),
        json!({ "ok": { "symbols" : [], "object_metadata": {"test_escaping":"\u{8}\u{C}\u{30}"} }}),
    );

    object_004.append_child(object_005, json!({}));
    object_001.append_child(object_002, json!({"name": "evil.dll", "x": [1,2,3]}));
    object_001.append_child(object_003, json!({"name": "evil.link", "x": "y"}));
    object_001.append_child(object_004, json!({"name": "index.html", "x": {}}));

    Work::new(
        "work_001",
        "my_org",
        datetime!(2000-01-01 01:00:00),
        object_001,
        json!({"name":"archive.zip", "names": ["sample5"], "nested": {"key":"value"}}),
    )
}

fn create_work_002() -> Work {
    let mut object_001 = Object::new(
        "object_002_001",
        "Zip",
        None,
        1_000_000,
        json!({"md5":"cc4bbf7d8d6f7671d1bf1f693b7d65dd", "sha1":"353045b208d64bb8c2fad710baf9bed9db3d3c83", "sha256":"96d76995c4f83e485e96d08ba12c4b75bb5797479c3a54b12c8f1768c9af5ce2"}),
        json!({ "ok": { "symbols" : [], "object_metadata": {} }}),
    );
    let object_002 = Object::new(
        "object_002_002",
        "Image",
        Some("BMP"),
        500_000,
        json!({"md5":"a07d4b3a65c0f9557d56c355297bf388", "sha1":"f1f62e9da57219de1b0c51709f190f6257ebcb8b", "sha256":"cde944648ce4d01ade1afe35f113ede1fdc452b0978c9ed1c16464f4a24ab2a0"}),
        json!({ "ok": { "symbols" : [], "object_metadata": {} }}),
    );
    let object_003 = Object::new(
        "object_002_003",
        "Image",
        Some("PNG"),
        300_000,
        json!({"md5":"ebb125b2c66b8c7005594e781ecdfcd5", "sha1":"4f3e6d5bdcdab30159c2fce95c2d4b7504421b26", "sha256":"5f06d88eace76e4e327758a1cf6979b7a92bb7b1056d2125c87c9e665a08c0b6"}),
        json!({ "ok": { "symbols" : ["B", "C"], "object_metadata": {} }}),
    );
    let object_004 = Object::new(
        "object_002_004",
        "Image",
        Some("PNG"),
        350_000,
        json!({"md5":"09c18792952797fc1028f6f503f0cd04", "sha1":"e14129a18b286c54f9f64abe824a19034e7e3911", "sha256":"86a3db94fd47167ba4915bd423dbe504f9a4b4ccf98f12e15541f681b647a433"}),
        json!({ "ok": { "symbols" : [], "object_metadata": {"array":[{"key": "From", "value":"B"},{"key": "To", "value":"Ctx"},{"key": "Subject", "value":"A"}]} }}),
    );
    let object_005 = Object::new(
        "object_002_005",
        "Image",
        Some("JPEG"),
        200_000,
        json!({"md5":"3b6cf0a20287994ceb09b0a1fb9253bc", "sha1":"355102115b8393a322ed46a3efdc99202e995779", "sha256":"dcacfab82aa9780a156f433ee2b58093c3a48b0221165d3d4103dba9c6e2d730"}),
        json!({ "ok": { "symbols" : [], "object_metadata": {} }}),
    );
    let object_006 = Object::new(
        "object_002_006",
        "Image",
        Some("JPEG"),
        250_000,
        json!({"md5":"5a9662f7be63353c31cfa75ae4613775", "sha1":"366342d1a906a98f9d82feefb619e65d8ae798e7", "sha256":"905b4d5147864c5382d3929b7028a7b32e355b80eafb37db96ab54368e1b5ae3"}),
        json!({ "ok": { "symbols" : [], "object_metadata": {} }}),
    );
    let object_007 = Object::new(
        "object_002_007",
        "Image",
        Some("JPEG"),
        275_000,
        json!({"md5":"2ad351f6592c67394cd80764da92c6de", "sha1":"2a4e59407a4deb1077a6ee8bc621a4a1aa70a5bd", "sha256":"e913bf63bede609636348e254dbd23653b0eac090b5c8ea2bed254c1c0e13228"}),
        json!({ "ok": { "symbols" : [], "object_metadata": {} }}),
    );
    object_001.append_child(
        object_002,
        json!({"name": "logo.bmp", "nested": {"key":"VALUE"}}),
    );
    object_001.append_child(object_003, json!({"name": "baner1.png"}));
    object_001.append_child(object_004, json!({"name": "baner2.png"}));
    object_001.append_child(object_005, json!({"name": "picture1.jpeg"}));
    object_001.append_child(object_006, json!({"name": "picture2.jpeg"}));
    object_001.append_child(object_007, json!({"name": "picture3.jpeg"}));
    Work::new(
        "work_002",
        "my_org",
        datetime!(2000-01-02 01:00:00),
        object_001,
        json!({"name":"images.zip"}),
    )
}

fn create_work_003() -> Work {
    let mut object_001 = Object::new(
        "object_003_001",
        "Zip",
        None,
        999_999,
        json!({"md5":"4ec77c894ab6d1133142eb4e377e68d8", "sha1":"15e783970b0fca4aa303332b57b0bce2fe00d544", "sha256":"3f7cac199350e5a2e9aabd5cbd09827255ce81ca4bf82ce5c25f1f32c0f4a723"}),
        json!({ "ok": { "symbols" : [], "object_metadata": {} }}),
    );
    let mut object_002 = Object::new(
        "object_003_002",
        "Zip",
        None,
        2_000_000,
        json!({"md5":"be7b6526bdf46f32d5a1bed0779a778e", "sha1":"7cf73cf88c0132933ceba87637d5250e6fbbf096", "sha256":"8897adcb4ce1e2b087f676b4a31e7f0ebd9ac0553f85f43afaac59e8b01057e6"}),
        json!({ "ok": { "symbols" : [], "object_metadata": {} }}),
    );
    let object_003 = Object::new(
        "object_003_003",
        "UNKNOWN",
        None,
        999_999,
        json!({"md5":"d2634edf082e2c73e8d0da8a1d755cb0", "sha1":"6edce65aa8716888b401e32d95f306a2b0d2586c", "sha256":"1967b26b25f317238942c8de93baedac924d20e8a86294d2f74dcca937e8a814"}),
        json!({ "ok": { "symbols" : [], "object_metadata": {} }}),
    );
    let object_004 = Object::new(
        "object_003_004",
        "UNKNOWN",
        None,
        888_888,
        json!({"md5":"7744635099fa78ffefeccbe4e5313c79", "sha1":"cf687c793904c3e30017c34dc3a05b926be9e413", "sha256":"d844b4b838fbfff4a0a1a87736c26582150adfc32a64047dd5362b46325c4d6e"}),
        json!({ "ok": { "symbols" : [], "object_metadata": {} }}),
    );
    let object_005: Object = Object::new(
        "object_003_005",
        "UNKNOWN",
        None,
        777_777,
        json!({"md5":"b25670a77b4cec40f9374b355e82349b", "sha1":"3910a45a5cd3a1b4c4fd52028fa7797585fd0d30", "sha256":"adf8d14eff4ef5ffb2fcbf95581005bf4c2152a5f8bb57c6285a150c04d94939"}),
        json!({ "ok": { "symbols" : [], "object_metadata": {} }}),
    );
    let object_006 = Object::new(
        "object_003_006",
        "UNKNOWN",
        None,
        666_666,
        json!({"md5":"15c017cba48bb8b95200bd73320e6c4b", "sha1":"baad2ab0fee2fd8f6f719afbb70b58ee0e70a06f", "sha256":"e5a8cdc6e7331fb3698b711a4fe66e85a2523addd7fba784308638da6c43e494"}),
        json!({ "ok": { "symbols" : [], "object_metadata": {} }}),
    );
    let object_007 = Object::new(
        "object_003_007",
        "UNKNOWN",
        None,
        555_555,
        json!({"md5":"b9af3a9e0d505c6abad01dd2c83ee4fc", "sha1":"80c0924aaaea982e29850d23b7f16ff4ddd280a9", "sha256":"b980e84a70cef9659d6a2f65f02edcfd9cd8af3a271bb5c26013e4f9f39b5eb8"}),
        json!({ "ok": { "symbols" : ["A"], "object_metadata": {} }}),
    );
    let mut object_008 = Object::new(
        "object_003_008",
        "Zip",
        None,
        1_234,
        json!({"md5":"6c8c75865177374b2c0581825606ecc6", "sha1":"b7962137cd69017b9a27e8eaac689c2f9ccb9fde", "sha256":"98c8bfbfad1b1e81e814e7800ea68ea8e05ac1bd652228c0213d8c8710a4f06c"}),
        json!({ "ok": { "symbols" : ["B"], "object_metadata": {"int1":1, "int2":1, "bool": true, "string": "string", "array": [1,2,3]} }}),
    );
    let object_009 = Object::new(
        "object_003_009",
        "Text",
        None,
        1_000,
        json!({"md5":"d517c3ed7b2ae9292fe99b135183fb21", "sha1":"1cb6af4e53e459dfc46292f2d7c0c20587e6664b", "sha256":"9ab0ab904d67487ab7404dc6f7c00c92a08c82d1e566aef4f641dfd4d6fe2e4d"}),
        json!({ "ok": { "symbols" : ["C"], "object_metadata": {} }}),
    );
    let object_010 = Object::new(
        "object_003_010",
        "Text",
        None,
        1_500,
        json!({"md5":"bb5b8d1ca0d8d142bdb3355bddc5da55", "sha1":"d0a608f960cc23b9f241af9f00e1383d84be7674", "sha256":"dd36c2348ad9ba8e3effedcf9f4cb3f0562da1df4dfed025d518546227503333"}),
        json!({ "ok": { "symbols" : [], "object_metadata": {"int1":3, "bool": false, "string": "string2", "array": [3,4,5]} }}),
    );
    let object_011 = Object::new(
        "object_003_011",
        "Text",
        None,
        2_000,
        json!({"md5":"48194baf245cbebd5f53e633ba2e864a", "sha1":"709e060caa59c3dbac6c477227972e8efe9221b8", "sha256":"e2e55df51b78841be4f59004c9485d1748d4b66a608d1c6b4bef4f12939d795f"}),
        json!({ "ok": { "symbols" : [], "object_metadata": {} }}),
    );
    let object_012 = Object::new(
        "object_003_012",
        "Text",
        None,
        2_500,
        json!({"md5":"14c937fecec83b9244113edbb824af96", "sha1":"fc5826062381109ef2f23f3e0c256220e9bcf524", "sha256":"31e58685014ff1034ad8e3393c834bf8ca5fecd581f9680a7f3354e53edd8ebe"}),
        json!({ "ok": { "symbols" : [], "object_metadata": {} }}),
    );
    let object_013 = Object::new(
        "object_003_013",
        "Text",
        None,
        3_000,
        json!({"md5":"a48f630c5c4823ce8e03e119511c9863", "sha1":"1e94da641e67aa07c4bc9359fe6df828df57a313", "sha256":"16179607d57df69a1e5c65ae9cb61074e0acc69e56e5ab011e1a6775192e5f82"}),
        json!({ "ok": { "symbols" : [], "object_metadata": {} }}),
    );
    object_002.append_child(object_003, json!({"name": "sample1", "nested": {"key":1}}));
    object_002.append_child(object_004, json!({"name": "sample2"}));
    object_002.append_child(object_005, json!({"name": "sample3"}));
    object_002.append_child(object_006, json!({"name": "sample4", "x": [1,2,3]}));
    object_002.append_child(object_007, json!({"name": "sample5"}));
    object_008.append_child(object_009, json!({"name": "README"}));
    object_008.append_child(
        object_010,
        json!({"name": "main.cpp", "nested": {"key":"1", "key2":"1"}}),
    );
    object_008.append_child(object_011, json!({"name": "include/misc.hpp"}));
    object_008.append_child(object_012, json!({"name": "include/database.hpp"}));
    object_008.append_child(object_013, json!({"name": "include/window.hpp"}));
    object_001.append_child(object_002, json!({"name": "src/database.cpp"}));
    object_001.append_child(
        object_008,
        json!({"name": "src/window.cpp", "nested": {"key":2}}),
    );
    Work::new(
        "work_003",
        "\"The Company\"",
        datetime!(2000-01-03 01:00:00),
        object_001,
        json!({"name":"project.zip"}),
    )
}

fn create_work_004() -> Work {
    let mut object_001 = Object::new(
        "object_004_001",
        "Rar",
        None,
        2_000,
        json!({"md5":"46a38c1d95187be8682a3c7c21038068", "sha1":"82aca88fae19ca6b24767fde9100f3d2042db9c0", "sha256":"8a521b352b1a5c46612bc29de16a5f3aea8fa48d788bdff20cd5d24473bfdab8"}),
        json!({ "ok": { "symbols" : ["TOP_SECRET"], "object_metadata": {} }}),
    );
    let object_002 = Object::new(
        "object_004_002",
        "PE",
        None,
        500,
        json!({"md5":"c37d895fc01b3405f44f1973a56ae68b", "sha1":"0fb9ed54f355362d5f7733b56e1e44ef401cddcc", "sha256":"b76b3541779fa8c07a95c965aa34404944ebe5245d5528c2e0b7e4781d8c5e73"}),
        json!({ "ok": { "symbols" : [], "object_metadata": {} }}),
    );
    let object_003 = Object::new(
        "object_004_003",
        "PE",
        None,
        1000,
        json!({"md5":"fe0ef0668b5c134bc06a66066eb8403f", "sha1":"d1e01467d66d9fea9cf3346fc1ea1606da3901e7", "sha256":"e02a1bec810994c9937b9c44be4faafafe98101c9895f58a33f5c3f6dcd08293"}),
        json!({ "ok": { "symbols" : [], "object_metadata": {} }}),
    );
    let object_004 = Object::new(
        "object_004_004",
        "Image",
        None,
        2000,
        json!({"md5":"f793745d5c301fdf260c41f95bd4d7e9", "sha1":"3f6e07e88fafae7ef70b540184fe0eed537bc213", "sha256":"a599a29fb409a3378e83eca884c0be9a98a03299ddbb368058e1c9b693de5472"}),
        json!({ "ok": { "symbols" : [], "object_metadata": {"array":[{"key": "From", "value":1},{"key": "To", "value":2},{"key": "Subject", "value":"Ctx"}]} }}),
    );
    let object_005 = Object::new(
        "object_004_005",
        "UNKNOWN",
        None,
        500,
        json!({"md5":"833fa252cb80d83aef857c45eb854933", "sha1":"48ac78a4102e5e824e576dfcb09808fdf5d429b6", "sha256":"8bc9a7809fbea44ee025cc04d493aab2e5f93df5749a61e24b92a71ed4c6ba03"}),
        json!({ "ok": { "symbols" : [], "object_metadata": {} }}),
    );
    object_001.append_child(object_002, json!({"name": "mario.EXE"}));
    object_001.append_child(object_003, json!({"name": "mario.dll"}));
    object_001.append_child(object_004, json!({"name": "assets.png", "x":"y"}));
    object_001.append_child(
        object_005,
        json!({"name": "data.bin", "nested": {"key":null}}),
    );
    Work::new(
        "work_004",
        "żółte\u{1F332}",
        datetime!(2000-01-04 01:00:00),
        object_001,
        json!({"name":"mario.rar"}),
    )
}

fn create_work_005() -> Work {
    let object_001 = Object::new(
        "object_005_001",
        "7z",
        None,
        2_000,
        json!({"md5":"f97b64685fea0c632d02782d9aa75361", "sha1":"f346cb827a620936fc030a8a560484990e40164f", "sha256":"cfd13765ff48f06fe632392d5df34123cd54a341f68b5a4cea2e327d6937bcf4"}),
        json!({ "error": "Invalid header"}),
    );

    Work::new(
        "work_005",
        "one\ttwo\nthree",
        datetime!(2000-01-05 01:00:00),
        object_001,
        json!({"name":"broken.7z", "names": ["README"], "x": {}}),
    )
}

pub struct Work {
    pub work_id: String,
    pub creation_time: PrimitiveDateTime,
    pub org: String,
    pub root: Object,
}

impl Work {
    fn new(
        work_id: &str,
        org: &str,
        creation_time: PrimitiveDateTime,
        mut root: Object,
        relation: serde_json::Value,
    ) -> Self {
        root.relation = relation;
        Self {
            work_id: work_id.to_string(),
            creation_time,
            org: org.to_string(),
            root,
        }
    }
}

pub struct Object {
    pub object_id: String,
    pub object_type: String,
    pub object_subtype: Option<String>,
    pub size: i64,
    pub hashes: serde_json::Value,
    pub result: serde_json::Value,
    pub relation: serde_json::Value,
    pub children: Vec<Object>,
}

impl Object {
    pub fn new(
        object_id: &str,
        object_type: &str,
        object_subtype: Option<&str>,
        size: i64,
        hashes: serde_json::Value,
        result: serde_json::Value,
    ) -> Self {
        Self {
            object_id: object_id.to_string(),
            object_type: object_type.to_string(),
            object_subtype: object_subtype.map(|s| s.to_string()),
            size,
            hashes,
            result,
            relation: json!({}),
            children: Vec::new(),
        }
    }
    pub fn append_child(&mut self, mut object: Object, relation: serde_json::Value) -> &mut Object {
        object.relation = relation;
        self.children.push(object);
        self.children.last_mut().unwrap()
    }
}
