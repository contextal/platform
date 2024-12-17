//! Text backend
use backend_utils::objects::{
    BackendRequest, BackendResultChild, BackendResultKind, BackendResultOk,
};
use chardetng::EncodingDetector;
use regex::Regex;
use serde::Serialize;
use simple_word_count::word_count;
use std::{
    collections::{HashMap, HashSet},
    env,
    fs::{self, File},
    io::{ErrorKind, Write},
    path::{Path, PathBuf},
};
use tensorflow::{
    Graph, Operation, SavedModelBundle, SessionOptions, SessionRunArgs, Tensor, CLASSIFY_INPUTS,
    CLASSIFY_OUTPUT_CLASSES, CLASSIFY_OUTPUT_SCORES, DEFAULT_SERVING_SIGNATURE_DEF_KEY,
};
use text_rs::{config::Config, TextBackendError};
use tracing::{error, info, instrument, trace, warn};
use tracing_subscriber::prelude::*;
use tree_sitter::{Language, Parser};
use tree_sitter_traversal::{traverse, Order};
use vader_sentiment::SentimentIntensityAnalyzer;
use whatlang::{Detector, Lang};

fn main() -> Result<(), TextBackendError> {
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let config = Config::new()?;
    if let Err(e) = tensorflow::library::load() {
        panic!("Failed to load tensorflow: {}", e);
    }

    let backend_state = BackendState::new(config)?;

    backend_utils::work_loop!(None, None, |request| {
        process_request(request, &backend_state)
    })?;

    Ok(())
}

/// Attributes of an input text.
#[derive(Debug, Serialize)]
struct TextMetadata<'a> {
    /// Identified character encoding
    encoding: String,

    /// Identified programming (or markup) language.
    programming_language: Option<String>,

    /// Identified natural language (whatlang)
    natural_language: Option<&'static str>,

    /// Language sentiment scores (only for English)
    natural_language_sentiment: Option<HashMap<&'a str, f64>>,

    /// Profanity count (only for English)
    natural_language_profanity_count: Option<usize>,

    /// Number of Unicode characters.
    number_of_characters: usize,

    /// Number of characters representing decimal digits.
    number_of_digits: usize,

    /// Number of characters which belong to ASCII symbols range.
    number_of_ascii_range_chars: usize,

    /// Number of whitespace (according to Unicode Character Database) characters.
    number_of_whitespaces: usize,

    /// Number of newlines
    number_of_newlines: usize,

    /// Number of words
    number_of_words: usize,

    /// Collection of URIs found in the input text.
    uris: Vec<String>,

    /// Collection of possible passwords "mentioned" in the input text.
    possible_passwords: Vec<String>,

    /// Hosts extracted for URLs
    unique_hosts: Vec<String>,

    /// Domains extracted for URLs
    unique_domains: Vec<String>,
}

#[derive(Serialize)]
pub struct UrlMetadata {
    /// URL
    pub url: String,
}

/// A container to keep initialized entities used by the backend during its life time.
struct BackendState<'a> {
    /// Backend configuration.
    config: Config,

    /// Machine learning model bundle loaded from disk.
    bundle: SavedModelBundle,

    /// Tensorflow model input operation.
    input_op: Operation,

    /// Tensorflow model output operation to fetch predicted scores.
    output_op_scores: Operation,

    /// Tensorflow model output operation to fetch prediction classes.
    output_op_classes: Operation,

    /// Instantiated natural language detector (whatlang).
    natural_language_detector: Detector,

    /// Instantiated sentiment analyzer
    sentiment_analyzer: SentimentIntensityAnalyzer<'a>,

    /// Compiled regular expression to find URIs.
    re_uri: Regex,

    /// Compiled regular expression to find URIs (basic version).
    re_uri_basic: Regex,

    /// Compiled regular expression to find what looks like credit card numbers.
    re_cc: Regex,

    /// Compiled regular expression to find mentioned passwords.
    re_password: Regex,

    /// Compiled regular expression to find profanity words.
    re_profanities: Regex,

    /// A collection of languages which could be verified by means of tree-sitter.
    languages_and_grammars: HashMap<&'static str, (&'static str, tree_sitter::Language)>,
}

impl BackendState<'_> {
    pub fn new(config: Config) -> Result<Self, TextBackendError> {
        let possible_model_locations =
            vec![concat!(env!("CARGO_MANIFEST_DIR"), "/model"), "/model"];
        let model_dir = match possible_model_locations
            .iter()
            .find(|model_dir| Path::new(&format!("{model_dir}/saved_model.pb")).exists())
        {
            Some(v) => v,
            None => Err(TextBackendError::GuesslangModelNotFound {
                locations: possible_model_locations,
            })?,
        };

        // Reduce Tensorflow library verbosity if it has not bee explicitly set already:
        if env::var("TF_CPP_MIN_LOG_LEVEL").is_err() {
            env::set_var("TF_CPP_MIN_LOG_LEVEL", "2")
        }

        let mut graph = Graph::new();
        let bundle =
            SavedModelBundle::load(&SessionOptions::new(), ["serve"], &mut graph, model_dir)?;

        let signature = bundle
            .meta_graph_def()
            .get_signature(DEFAULT_SERVING_SIGNATURE_DEF_KEY)?;
        let input_info = signature.get_input(CLASSIFY_INPUTS)?;
        let input_op = graph.operation_by_name_required(&input_info.name().name)?;
        let output_info_scores = signature.get_output(CLASSIFY_OUTPUT_SCORES)?;
        let output_op_scores = graph.operation_by_name_required(&output_info_scores.name().name)?;
        let output_info_classes = signature.get_output(CLASSIFY_OUTPUT_CLASSES)?;
        let output_op_classes =
            graph.operation_by_name_required(&output_info_classes.name().name)?;

        let natural_language_detector = Detector::new();

        let sentiment_analyzer = SentimentIntensityAnalyzer::new();

        // This is mostly https://daringfireball.net/2010/07/improved_regex_for_matching_urls with
        // added explicit IANA's URI Schemes from
        // https://www.iana.org/assignments/uri-schemes/uri-schemes.txt as of 2023-11-28:
        let re_uri = Regex::new(
            r#"(?xi)
            \b
            (?P<uri>
              (?:
                (?:aaa|aaas|about|acap|acct|acd|acr|adiumxtra|adt|afp|afs|aim|amss|android|appdata
                  |apt|ar|ark|at|attachment|aw|barion|bb|beshare|bitcoin|bitcoincash|blob|bolo
                  |brid|browserext|cabal|calculator|callto|cap|cast|casts|chrome|chrome\-extension
                  |cid|coap|coap\+tcp|coap\+ws|coaps|coaps\+tcp|coaps\+ws
                  |com\-eventbrite\-attendee|content|content\-type|crid|cstr|cvs|dab|dat|data|dav
                  |dhttp|diaspora|dict|did|dis|dlna\-playcontainer|dlna\-playsingle|dns|dntp|doi
                  |dpp|drm|drop|dtmi|dtn|dvb|dvx|dweb|ed2k|eid|elsi|embedded|ens|ethereum|example
                  |facetime|fax|feed|feedready|fido|file|filesystem|finger
                  |first\-run\-pen\-experience|fish|fm|ftp|fuchsia\-pkg|geo|gg|git|gitoid
                  |gizmoproject|go|gopher|graph|grd|gtalk|h323|ham|hcap|hcp|http|https|hxxp|hxxps
                  |hydrazone|hyper|iax|icap|icon|im|imap|info|iotdisco|ipfs|ipn|ipns|ipp|ipps|irc
                  |irc6|ircs|iris|iris\.beep|iris\.lwz|iris\.xpc|iris\.xpcs|isostore|itms|jabber
                  |jar|jms|keyparc|lastfm|lbry|ldap|ldaps|leaptofrogans|lid|lorawan|lpa|lvlt
                  |magnet|mailserver|mailto|maps|market|matrix|message|microsoft\.windows\.camera
                  |microsoft\.windows\.camera\.multipicker|microsoft\.windows\.camera\.picker|mid
                  |mms|modem|mongodb|moz|ms\-access|ms\-appinstaller|ms\-browser\-extension
                  |ms\-calculator|ms\-drive\-to|ms\-enrollment|ms\-excel|ms\-eyecontrolspeech
                  |ms\-gamebarservices|ms\-gamingoverlay|ms\-getoffice|ms\-help|ms\-infopath
                  |ms\-inputapp|ms\-launchremotedesktop|ms\-lockscreencomponent\-config
                  |ms\-media\-stream\-id|ms\-meetnow|ms\-mixedrealitycapture|ms\-mobileplans
                  |ms\-newsandinterests|ms\-officeapp|ms\-people|ms\-project|ms\-powerpoint
                  |ms\-publisher|ms\-remotedesktop|ms\-remotedesktop\-launch
                  |ms\-restoretabcompanion|ms\-screenclip|ms\-screensketch|ms\-search
                  |ms\-search\-repair|ms\-secondary\-screen\-controller
                  |ms\-secondary\-screen\-setup|ms\-settings|ms\-settings\-airplanemode
                  |ms\-settings\-bluetooth|ms\-settings\-camera|ms\-settings\-cellular
                  |ms\-settings\-cloudstorage|ms\-settings\-connectabledevices
                  |ms\-settings\-displays\-topology|ms\-settings\-emailandaccounts
                  |ms\-settings\-language|ms\-settings\-location|ms\-settings\-lock
                  |ms\-settings\-nfctransactions|ms\-settings\-notifications|ms\-settings\-power
                  |ms\-settings\-privacy|ms\-settings\-proximity|ms\-settings\-screenrotation
                  |ms\-settings\-wifi|ms\-settings\-workplace|ms\-spd|ms\-stickers|ms\-sttoverlay
                  |ms\-transit\-to|ms\-useractivityset|ms\-virtualtouchpad|ms\-visio|ms\-walk\-to
                  |ms\-whiteboard|ms\-whiteboard\-cmd|ms\-word|msnim|msrp|msrps|mss|mt|mtqp|mumble
                  |mupdate|mvn|news|nfs|ni|nih|nntp|notes|num|ocf|oid|onenote|onenote\-cmd
                  |opaquelocktoken|openid|openpgp4fpr|otpauth|p1|pack|palm|paparazzi|payment|payto
                  |pkcs11|platform|pop|pres|prospero|proxy|pwid|psyc|pttp|qb|query|quic\-transport
                  |redis|rediss|reload|res|resource|rmi|rsync|rtmfp|rtmp|rtsp|rtsps|rtspu|sarif
                  |secondlife|secret\-token|service|session|sftp|sgn|shc|shttp|sieve|simpleledger
                  |simplex|sip|sips|skype|smb|smp|sms|smtp|snews|snmp|soap\.beep|soap\.beeps
                  |soldat|spiffe|spotify|ssb|ssh|starknet|steam|stun|stuns|submit|svn|swh|swid
                  |swidpath|tag|taler|teamspeak|tel|teliaeid|telnet|tftp|things|thismessage|tip
                  |tn3270|tool|turn|turns|tv|udp|unreal|upt|urn|ut2004|uuid\-in\-package|v\-event
                  |vemmi|ventrilo|ves|videotex|vnc|view\-source|vscode|vscode\-insiders|vsls|w3
                  |wais|web3|wcr|webcal|web\+ap|wifi|wpid|ws|wss|wtai|wyciwyg|xcon|xcon\-userid
                  |xfire|xmlrpc\.beep|xmlrpc\.beeps|xmpp|xri|ymsgr|z39\.50|z39\.50r
                  |z39\.50s):                       # URL scheme and a colon
                (?:
                  /{1,3}                            # 1-3 slashes
                  |[a-z0-9%]                        # or single letter or digit or '%' (trying
                                                    # not to match e.g. "URI::Escape")
                )
                |www\d{0,3}[.]                      # or "www.", "www1.", "www2." … "www999."
                |[a-z0-9.-]+[.][a-z]{2,4}/          # or looks like domain name followed by a slash
              )
              (?:                                   # One or more:
                [^\s()<>]+                          # Run of non-space, non-()<>
                |\(([^\s()<>]+|(\([^\s()<>]+\)))*\) # or balanced parens, up to 2 levels
              )+
              (?:                                   # End with:
                \(([^\s()<>]+|(\([^\s()<>]+\)))*\)  # balanced parens, up to 2 levels
                |[^\s`!()\[\]{};:'".,<>?«»“”‘’]     # or not a space or one of these punct char
              )
            )                                       # End of `uri` capture group
            "#,
        )
        .expect("invalid URI regex");

        let re_uri_basic = Regex::new(
            r#"(?xi)
            \b
            (?P<uri>
              (?:
                (?:
                  bitcoin|bitcoincash|browserext|chrome|chrome\-extension
                  |ethereum|ftp|http|https|hxxp|hxxps|magnet|mailto|microsoft\.windows\.camera
                  |microsoft\.windows\.camera\.multipicker|microsoft\.windows\.camera\.picker|mid
                  |moz|ms\-access|ms\-appinstaller|ms\-browser\-extension
                  |ms\-calculator|ms\-drive\-to|ms\-enrollment|ms\-excel|ms\-eyecontrolspeech
                  |ms\-gamebarservices|ms\-gamingoverlay|ms\-getoffice|ms\-help|ms\-infopath
                  |ms\-inputapp|ms\-launchremotedesktop|ms\-lockscreencomponent\-config
                  |ms\-media\-stream\-id|ms\-meetnow|ms\-mixedrealitycapture|ms\-mobileplans
                  |ms\-newsandinterests|ms\-officeapp|ms\-people|ms\-project|ms\-powerpoint
                  |ms\-publisher|ms\-remotedesktop|ms\-remotedesktop\-launch
                  |ms\-restoretabcompanion|ms\-screenclip|ms\-screensketch|ms\-search
                  |ms\-search\-repair|ms\-secondary\-screen\-controller
                  |ms\-secondary\-screen\-setup|ms\-settings|ms\-settings\-airplanemode
                  |ms\-settings\-bluetooth|ms\-settings\-camera|ms\-settings\-cellular
                  |ms\-settings\-cloudstorage|ms\-settings\-connectabledevices
                  |ms\-settings\-displays\-topology|ms\-settings\-emailandaccounts
                  |ms\-settings\-language|ms\-settings\-location|ms\-settings\-lock
                  |ms\-settings\-nfctransactions|ms\-settings\-notifications|ms\-settings\-power
                  |ms\-settings\-privacy|ms\-settings\-proximity|ms\-settings\-screenrotation
                  |ms\-settings\-wifi|ms\-settings\-workplace|ms\-spd|ms\-stickers|ms\-sttoverlay
                  |ms\-transit\-to|ms\-useractivityset|ms\-virtualtouchpad|ms\-visio|ms\-walk\-to
                  |ms\-whiteboard|ms\-whiteboard\-cmd|ms\-word
                  |onenote\-cmd|rsync|sftp|shttp|smb|ssh|tftp
                  ):                       # URL scheme and a colon
                (?:
                  /{1,3}                            # 1-3 slashes
                  |[a-z0-9%]                        # or single letter or digit or '%' (trying
                                                    # not to match e.g. "URI::Escape")
                )
                |www\d{0,3}[.]                      # or "www.", "www1.", "www2." … "www999."
              )
              (?:                                   # One or more:
                [^\s()<>]+                          # Run of non-space, non-()<>
                |\(([^\s()<>]+|(\([^\s()<>]+\)))*\) # or balanced parens, up to 2 levels
              )+
              (?:                                   # End with:
                \(([^\s()<>]+|(\([^\s()<>]+\)))*\)  # balanced parens, up to 2 levels
                |[^\s`!()\[\]{};:'".,<>?«»“”‘’]     # or not a space or one of these punct char
              )
            )                                       # End of `uri` capture group
            "#,
        )
        .expect("invalid URI regex (basic)");

        let re_cc = Regex::new(r#"\b((?:\d{4}[\s-]?){4})\b"#).expect("invalid CC regex");
        let re_password = Regex::new(
            r#"(?xi)
                \b((
                    (:?password|passphrase|passcode)    # an opening word
                    \s?                                 # an optional space
                    (:?[:-]|is)?                        # an optional colon, dash or "is"
                )|(
                    (pass|key|code)                     # an opening word
                    \s?                                 # an optional space
                    (:?[:]|is)                          # colon or "is"
                ))
                \s                                      # a space
                (?P<password>[^\s]+)                    # the password itself as a sequence of
                                                        # non-space symbols
                (\n|$)                                  # password must be last word in the line
            "#,
        )
        .expect("invalid password regex");

        let re_profanities = Regex::new(
            r#"(?xi)\b
	    (anal|anus|arse|ass|asshole|ballsack|bastard|biatch
	    |bitch|bloody|blowjob|bollock|bollok|boner|boob|boobie
	    |boobies|boobjob|breast|bugger|butt|buttplug|clitoris
	    |cock|condom|coon|crap|cunnilingus|cunt|damn|dick
	    |dildo|doggystyle|dyke|ejaculate|fuck|fag|faggot
	    |fagot|feck|felate|felatio|felching|fellate|fellatio
	    |fetish|flange|foreskin|fuck|fucked|fucking|fudgepacker
	    |fudgepacker|goddamn|handjob|jerk|jizz|knobend
	    |lmao|lmfao|masterbate|masterbation|masturbate
	    |masturbation|muff|nigga|nigger|penis|piss|poop|prick
	    |pube|pussy|queer|rimjob|scrotum|semen|sex|sh1t|shit
	    |slut|smegma|spunk|suck|sucks|tit|tits|tittie|titties
	    |titty|tosser|turd|twat|vagina|wank|whore|wtf)
	    \b"#,
        )
        .expect("invalid profanities regex");

        // The set of languages supported by `guesslang` ML model restricted to a much smaller set
        // below.
        //
        // The languages excluded from the original list are:
        // - compiled languages (because at the moment there is no any value in detecting these
        // languages), and
        // - languages for which there is no tree-sitter grammar available (what makes it impossible to
        // perform basic grammar verification to confirm or reject ML model prediction).
        let languages_and_grammars = HashMap::from([
            ("dart", ("Dart", tree_sitter_dart::language())),
            ("html", ("HTML", tree_sitter_html::language())),
            ("js", ("JavaScript", tree_sitter_javascript::language())),
            ("json", ("JSON", tree_sitter_json::language())),
            ("lua", ("Lua", tree_sitter_lua::language())),
            ("php", ("PHP", tree_sitter_php::language())),
            ("py", ("Python", tree_sitter_python::language())),
            ("rb", ("Ruby", tree_sitter_ruby::language())),
            ("sh", ("Bash", tree_sitter_bash::language())),
            ("toml", ("TOML", tree_sitter_toml::language())),
            (
                "ts",
                ("TypeScript", tree_sitter_typescript::language_typescript()),
            ),
        ]);

        Ok(Self {
            config,
            bundle,
            input_op,
            output_op_scores,
            output_op_classes,
            natural_language_detector,
            sentiment_analyzer,
            re_uri,
            re_uri_basic,
            re_cc,
            re_password,
            re_profanities,
            languages_and_grammars,
        })
    }
}

#[derive(Serialize)]
struct DomainMetadata {
    name: String,
}

#[instrument(level="error", skip_all, fields(object_id = request.object.object_id))]
fn process_request(
    request: &BackendRequest,
    backend_state: &BackendState,
) -> Result<BackendResultKind, TextBackendError> {
    let input_path =
        PathBuf::from(&backend_state.config.objects_path).join(&request.object.object_id);
    let file = File::open(&input_path).map_err(|e| {
        error!("failed to open an input file {input_path:?}: {e:?}");
        e
    })?;
    let file_size = file
        .metadata()
        .map_err(|e| {
            error!("failed to read an input file metadata: {e:?}");
            e
        })?
        .len();
    if file_size > backend_state.config.max_processed_size {
        let message = format!(
            "file size ({file_size}) exceeds the limit ({})",
            backend_state.config.max_processed_size
        );
        info!(message);
        return Ok(BackendResultKind::error(message));
    }
    let mut symbols = vec![];
    let mut detect_natural_language = true;
    let encoding;

    let text = match fs::read_to_string(&input_path) {
        Ok(v) => {
            encoding = String::from("utf-8");
            v
        }
        Err(e) if e.kind() == ErrorKind::InvalidData => {
            trace!("failed to read a file as UTF-8, falling back to encoding detection");
            // Disable language detection for non UTF-8 inputs
            detect_natural_language = false;

            let file_content = match fs::read(&input_path) {
                Ok(v) => v,
                Err(e) => {
                    let message = format!("failed to read an input file: {e:?}");
                    error!(message);
                    return Ok(BackendResultKind::error(message));
                }
            };
            let mut encoding_detector = EncodingDetector::new();
            encoding_detector.feed(&file_content, true);

            // If the data looks like UTF-8 from the detector perspective, so be it.
            // This would allow us to handle incomplete data which ends abruptly in the middle of
            // multi-byte Unicode character.
            const ALLOW_UTF8: bool = true;

            let guess = encoding_detector.guess(None, ALLOW_UTF8);
            let (decoded, _, had_errors) = guess.decode(&file_content);

            if had_errors {
                symbols.push("CHAR_DECODING_ERRORS".into());
            }

            encoding = guess.name().to_string();
            decoded.to_string()
        }
        Err(e) => {
            let message = format!("failed to read an input file to string: {e:?}");
            error!(message);
            return Ok(BackendResultKind::error(message));
        }
    };

    let input_tensor: Tensor<String> = match Tensor::new(&[1]).with_values(&[text.clone()]) {
        Ok(v) => v,
        Err(e) => {
            let message = format!("failed to construct a Tensor: {e:?}");
            error!(message);
            return Ok(BackendResultKind::error(message));
        }
    };

    let mut args = SessionRunArgs::new();
    args.add_feed(&backend_state.input_op, 0, &input_tensor);
    let fetch_scores = args.request_fetch(&backend_state.output_op_scores, 0);
    let fetch_classes = args.request_fetch(&backend_state.output_op_classes, 0);

    let session = &backend_state.bundle.session;
    if let Err(e) = session.run(&mut args) {
        let message = format!("failed to run an input tensor through a model: {e:?}");
        error!(message);
        return Ok(BackendResultKind::error(message));
    }

    let classes: Vec<String> = match args.fetch(fetch_classes) {
        Ok(v) => v.iter().cloned().collect(),
        Err(e) => {
            let message = format!("failed to fetch classification classes from ML model: {e:?}");
            error!(message);
            return Ok(BackendResultKind::error(message));
        }
    };
    let scores: Vec<f32> = match args.fetch(fetch_scores) {
        Ok(v) => v.iter().cloned().collect(),
        Err(e) => {
            let message = format!("failed to fetch classification scores from ML model: {e:?}");
            error!(message);
            return Ok(BackendResultKind::error(message));
        }
    };
    let mean = 1.0 / scores.len() as f32;
    let std_deviation = f32::sqrt(
        scores
            .iter()
            .map(|&score| (mean - score) * (mean - score))
            .sum::<f32>()
            / scores.len() as f32,
    );

    trace!(
        "`guesslang` model output: {:#?}",
        scores
            .iter()
            .zip(classes.iter())
            .map(|(confidence, class)| format!("{confidence:.02} {class}",))
            .collect::<Vec<_>>()
    );

    let mut model_propositions_filtered: Vec<(f32, &(&str, Language))> = scores
        .into_iter()
        .zip(classes)
        .filter_map(|(score, class)| {
            // Filter out ML model results which have probabilities below one standard deviation
            // above the mean.
            if score > mean + std_deviation {
                if let Some(language_and_grammar) =
                    backend_state.languages_and_grammars.get(class.as_str())
                {
                    return Some((score, language_and_grammar));
                }
            }
            None
        })
        .collect();
    model_propositions_filtered
        .sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or_else(|| unreachable!()));
    trace!(
        "`guesslang` model propositions filtered: {:?}",
        model_propositions_filtered
            .iter()
            .map(|(confidence, (language, _))| (confidence, language))
            .collect::<Vec<_>>()
    );

    let mut programming_language = if request.symbols.contains(&"VBA".to_owned()) {
        Some("Visual Basic for Applications".to_string())
    } else {
        model_propositions_filtered
            .into_iter()
            .find_map(|(_, (language_name, grammar))| {
                let mut parser = Parser::new();
                if let Err(e) = parser.set_language(*grammar) {
                    warn!("failed to load {language_name:?} grammar: {e:?}");
                    return None;
                }

                // Tree-sitter can actually stuck while parsing, so timeout is necessary:
                parser.set_timeout_micros(1_000_000);

                if let Some(parsed) = parser.parse(&text, None) {
                    let (parsed, comments, errors) = traverse(parsed.walk(), Order::Pre).fold(
                        (0, 0, 0),
                        |(parsed, comments, errors), node| {
                            (
                                // Don't account broad-and-generic nodes in "successfully parsed"
                                // counter. As this counter is later used as one of indicators of
                                // successfully guessed programming language.
                                parsed
                                    + (!["program", "document", "fragment", "text"]
                                        .contains(&node.kind()))
                                        as usize,
                                comments
                                    + (node.is_extra() && node.kind().contains("comment")) as usize,
                                errors + node.is_error() as usize,
                            )
                        },
                    );
                    trace!(
                        "tree-sitter traversal for {language_name}: \
			    parsed: {parsed}, error: {errors}"
                    );
                    if errors == 0 && parsed != 0 {
                        if comments == parsed {
                            symbols.push("CODE_ALL_COMMENTS".into());
                        }
                        Some(language_name.to_string())
                    } else {
                        None
                    }
                } else {
                    warn!("tree-sitter has failed to parse an input in allotted time frame");
                    None
                }
            })
    };

    let (
        number_of_characters,
        number_of_ascii_range_chars,
        number_of_digits,
        number_of_whitespaces,
        number_of_newlines,
    ) = text.chars().fold((0, 0, 0, 0, 0), |acc, v| {
        (
            acc.0 + 1,
            acc.1 + v.is_ascii() as usize,
            acc.2 + v.is_ascii_digit() as usize,
            acc.3 + v.is_whitespace() as usize,
            acc.4 + (v == '\n') as usize,
        )
    });
    let number_of_words = word_count(&text);

    if ((programming_language == Some("Bash".into()) && !text.starts_with("#!/bin/"))
        || programming_language == Some("Ruby".into()))
        && (number_of_newlines < 4 || number_of_digits == 0)
    {
        programming_language = None;
    }

    if programming_language.is_some()
        || (backend_state
            .config
            .natural_language_max_char_whitespace_ratio
            > 0.0
            && (number_of_whitespaces == 0
                || number_of_characters as f64 / number_of_whitespaces as f64
                    > backend_state
                        .config
                        .natural_language_max_char_whitespace_ratio))
    {
        detect_natural_language = false;
    }

    if number_of_digits > 0 {
        if number_of_digits == number_of_characters - number_of_whitespaces {
            detect_natural_language = false;
            symbols.push("ALL_NUMBERS".into())
        } else if number_of_digits > (number_of_characters - number_of_whitespaces) / 2 {
            detect_natural_language = false;
            symbols.push("MOSTLY_NUMBERS".into())
        } else if number_of_digits > (number_of_characters - number_of_whitespaces) / 10 {
            symbols.push("MANY_NUMBERS".into())
        }
    }

    if number_of_characters == number_of_ascii_range_chars {
        symbols.push("ALL_ASCII".into())
    }

    if number_of_newlines == 0 {
        detect_natural_language = false
    }

    let mut natural_language = None;
    if detect_natural_language {
        if let Some(info) = backend_state.natural_language_detector.detect(&text) {
            if info.confidence() >= backend_state.config.natural_language_min_confidence_level {
                natural_language = Some(info.lang());
            }
        }
    }

    let mut natural_language_sentiment = None;
    let mut natural_language_profanity_count = None;
    if natural_language == Some(Lang::Eng) {
        natural_language_sentiment = Some(backend_state.sentiment_analyzer.polarity_scores(&text));
        natural_language_profanity_count =
            Some(backend_state.re_profanities.find_iter(&text).count());
    }

    let re_uri = if programming_language.is_some() {
        &backend_state.re_uri_basic
    } else {
        &backend_state.re_uri
    };

    let mut uris = re_uri
        .captures_iter(&text)
        .map(|capture| {
            capture
                .name("uri")
                .expect("URI regex doesn't have an `uri` capture group")
                .as_str()
        })
        .collect::<HashSet<_>>()
        .into_iter()
        .map(String::from)
        .collect::<Vec<_>>();
    uris.sort_unstable();

    let mut possible_passwords = if programming_language.is_none() {
        backend_state
            .re_password
            .captures_iter(&text)
            .map(|capture| {
                capture
                    .name("password")
                    .expect("password regex doesn't have a `password` capture group")
                    .as_str()
            })
            .collect::<HashSet<_>>()
            .into_iter()
            .map(String::from)
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    possible_passwords.sort_unstable();

    if backend_state.re_cc.find_iter(&text).any(|matched| {
        card_validate::Validate::from(
            &matched
                .as_str()
                .chars()
                .filter(char::is_ascii_digit)
                .collect::<String>(),
        )
        .is_ok()
    }) {
        symbols.push("CC_NUMBER".into())
    }

    let mut children: Vec<BackendResultChild> = Vec::new();
    let mut unique_hosts: Vec<String> = Vec::new();
    let mut unique_domains: Vec<String> = Vec::new();
    let max_children = usize::try_from(backend_state.config.max_children).unwrap_or(usize::MAX);
    for uri in uris.iter() {
        if let Ok(url) = url::Url::parse(uri) {
            if ["http", "https"].contains(&url.scheme()) {
                if children.len() < max_children
                    && backend_state.config.create_url_children
                    && request.symbols.contains(&"OCR".to_owned())
                {
                    let mut url_file =
                        tempfile::NamedTempFile::new_in(&backend_state.config.output_path)?;
                    if url_file.write_all(&uri.clone().into_bytes()).is_ok() {
                        children.push(BackendResultChild {
                            path: Some(
                                url_file
                                    .into_temp_path()
                                    .keep()
                                    .unwrap()
                                    .into_os_string()
                                    .into_string()
                                    .unwrap(),
                            ),
                            force_type: Some("URL".to_string()),
                            symbols: vec![ /* "SHORT_URL".to_string() */ ],
                            relation_metadata: match serde_json::to_value(UrlMetadata {
                                url: uri.clone(),
                            })? {
                                serde_json::Value::Object(v) => v,
                                _ => unreachable!(),
                            },
                        });
                    }
                }
            }
            if let Some(host) = url.host_str() {
                unique_hosts.push(host.to_string());
                if let Ok(domain) = addr::parse_domain_name(host) {
                    if let Some(root) = domain.root() {
                        unique_domains.push(root.to_string());
                    }
                }
            }
        }
    }
    unique_hosts.sort_unstable();
    unique_hosts.dedup();
    unique_domains.sort_unstable();
    unique_domains.dedup();
    if backend_state.config.create_domain_children {
        for domain in unique_domains.iter() {
            if children.len() >= max_children {
                break;
            }
            let mut domain_file =
                tempfile::NamedTempFile::new_in(&backend_state.config.output_path)?;
            if domain_file.write_all(domain.as_bytes()).is_ok() {
                children.push(BackendResultChild {
                    path: Some(
                        domain_file
                            .into_temp_path()
                            .keep()
                            .unwrap()
                            .into_os_string()
                            .into_string()
                            .unwrap(),
                    ),
                    force_type: Some("Domain".to_string()),
                    symbols: vec![],
                    relation_metadata: match serde_json::to_value(DomainMetadata {
                        name: domain.clone(),
                    })? {
                        serde_json::Value::Object(v) => v,
                        _ => unreachable!(),
                    },
                });
            }
        }
    }
    if children.len() >= max_children {
        symbols.push("LIMITS_REACHED".into());
    }

    let metadata = TextMetadata {
        encoding,
        programming_language,
        natural_language: natural_language.map(|l| l.eng_name()),
        natural_language_profanity_count,
        natural_language_sentiment,
        number_of_characters,
        number_of_ascii_range_chars,
        number_of_digits,
        number_of_whitespaces,
        number_of_newlines,
        number_of_words,
        uris,
        possible_passwords,
        unique_hosts,
        unique_domains,
    };

    Ok(BackendResultKind::ok(BackendResultOk {
        symbols,
        object_metadata: match serde_json::to_value(metadata)? {
            serde_json::Value::Object(v) => v,
            _ => unreachable!(),
        },
        children,
    }))
}

#[cfg(test)]
mod test;
