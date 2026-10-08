//! LaTeX control sequence to Unicode codepoint and atom class.

use crate::class::AtomClass;

use AtomClass::{Bin, Close, Inner, Op, Open, Ord, Rel};

/// Both directions of the table, generated from one list of arms.
macro_rules! table {
    ($($first:literal $(| $alias:literal)* => ($ch:literal, $class:expr),)*) => {
        #[must_use]
        pub(crate) fn resolve(name: &str) -> Option<(char, AtomClass)> {
            Some(match name {
                $($first $(| $alias)* => ($ch, $class),)*
                _ => return None,
            })
        }

        /// The command that writes this character at this class; the first spelling wins.
        #[must_use]
        pub(crate) fn spell(ch: char, class: AtomClass) -> Option<&'static str> {
            $(if ch == $ch && class == $class {
                return Some($first);
            })*
            None
        }
    };
}

table! {
        // Greek, lowercase
        // Upright here; `Alphabet::Auto` italicises lowercase at layout time, as TeX does.
        "alpha" => ('\u{03B1}', Ord),
        "beta" => ('\u{03B2}', Ord),
        "gamma" => ('\u{03B3}', Ord),
        "delta" => ('\u{03B4}', Ord),
        // LaTeX's `\epsilon` is the lunate ϵ; `\varepsilon` is ε.
        "epsilon" => ('\u{03F5}', Ord),
        "varepsilon" => ('\u{03B5}', Ord),
        "zeta" => ('\u{03B6}', Ord),
        "eta" => ('\u{03B7}', Ord),
        "theta" => ('\u{03B8}', Ord),
        "vartheta" => ('\u{03D1}', Ord),
        "iota" => ('\u{03B9}', Ord),
        "kappa" => ('\u{03BA}', Ord),
        "lambda" => ('\u{03BB}', Ord),
        "mu" => ('\u{03BC}', Ord),
        "nu" => ('\u{03BD}', Ord),
        "xi" => ('\u{03BE}', Ord),
        "omicron" => ('\u{03BF}', Ord),
        "pi" => ('\u{03C0}', Ord),
        "varpi" => ('\u{03D6}', Ord),
        "rho" => ('\u{03C1}', Ord),
        "varrho" => ('\u{03F1}', Ord),
        "sigma" => ('\u{03C3}', Ord),
        "varsigma" => ('\u{03C2}', Ord),
        "tau" => ('\u{03C4}', Ord),
        "upsilon" => ('\u{03C5}', Ord),
        // Likewise `\phi` is the straight-stemmed ϕ.
        "phi" => ('\u{03D5}', Ord),
        "varphi" => ('\u{03C6}', Ord),
        "chi" => ('\u{03C7}', Ord),
        "psi" => ('\u{03C8}', Ord),
        "omega" => ('\u{03C9}', Ord),
        "digamma" => ('\u{03DD}', Ord),
        "varkappa" => ('\u{03F0}', Ord),

        // Greek, uppercase
        "Gamma" => ('\u{0393}', Ord),
        "Delta" => ('\u{0394}', Ord),
        "Theta" => ('\u{0398}', Ord),
        "Lambda" => ('\u{039B}', Ord),
        "Xi" => ('\u{039E}', Ord),
        "Pi" => ('\u{03A0}', Ord),
        "Sigma" => ('\u{03A3}', Ord),
        "Upsilon" => ('\u{03A5}', Ord),
        "Phi" => ('\u{03A6}', Ord),
        "Psi" => ('\u{03A8}', Ord),
        "Omega" => ('\u{03A9}', Ord),
        // The capitals that look Latin, as Greek codepoints so they stay upright.
        "Alpha" => ('\u{0391}', Ord),
        "Beta" => ('\u{0392}', Ord),
        "Epsilon" => ('\u{0395}', Ord),
        "Zeta" => ('\u{0396}', Ord),
        "Eta" => ('\u{0397}', Ord),
        "Iota" => ('\u{0399}', Ord),
        "Kappa" => ('\u{039A}', Ord),
        "Mu" => ('\u{039C}', Ord),
        "Nu" => ('\u{039D}', Ord),
        "Omicron" => ('\u{039F}', Ord),
        "Rho" => ('\u{03A1}', Ord),
        "Tau" => ('\u{03A4}', Ord),
        "Chi" => ('\u{03A7}', Ord),

        // Binary operators
        "cdot" | "centerdot" => ('\u{22C5}', Bin),
        "times" => ('\u{00D7}', Bin),
        "div" => ('\u{00F7}', Bin),
        "pm" => ('\u{00B1}', Bin),
        "mp" => ('\u{2213}', Bin),
        "oplus" => ('\u{2295}', Bin),
        "ominus" => ('\u{2296}', Bin),
        "otimes" => ('\u{2297}', Bin),
        "odot" => ('\u{2299}', Bin),
        "circ" => ('\u{2218}', Bin),
        "bullet" => ('\u{2219}', Bin),
        "star" => ('\u{22C6}', Bin),
        "ast" => ('\u{2217}', Bin),
        "setminus" => ('\u{2216}', Bin),
        "wedge" | "land" => ('\u{2227}', Bin),
        "vee" | "lor" => ('\u{2228}', Bin),
        "cap" => ('\u{2229}', Bin),
        "cup" => ('\u{222A}', Bin),
        // `\mathord` in TeX: a prefix, not an infix.
        "neg" | "lnot" => ('\u{00AC}', Ord),
        // Daggers are binary operators in TeX.
        "dagger" | "dag" => ('\u{2020}', Bin),
        "ddagger" | "ddag" => ('\u{2021}', Bin),
        "uplus" => ('\u{228E}', Bin),
        "sqcap" => ('\u{2293}', Bin),
        "sqcup" => ('\u{2294}', Bin),
        "amalg" => ('\u{2A3F}', Bin),
        "wr" => ('\u{2240}', Bin),
        "diamond" => ('\u{22C4}', Bin),
        "bigtriangleup" => ('\u{25B3}', Bin),
        "bigtriangledown" => ('\u{25BD}', Bin),
        "triangleleft" => ('\u{25C1}', Bin),
        "triangleright" => ('\u{25B7}', Bin),
        // Same characters as `\vartriangleleft` and friends, at a different class.
        "lhd" => ('\u{22B2}', Bin),
        "rhd" => ('\u{22B3}', Bin),
        "unlhd" => ('\u{22B4}', Bin),
        "unrhd" => ('\u{22B5}', Bin),
        "bigcirc" => ('\u{25EF}', Bin),
        "oslash" => ('\u{2298}', Bin),
        "circledast" => ('\u{229B}', Bin),
        "circledcirc" => ('\u{229A}', Bin),
        "circleddash" => ('\u{229D}', Bin),
        "boxplus" => ('\u{229E}', Bin),
        "boxminus" => ('\u{229F}', Bin),
        "boxtimes" => ('\u{22A0}', Bin),
        "boxdot" => ('\u{22A1}', Bin),
        "ltimes" => ('\u{22C9}', Bin),
        "rtimes" => ('\u{22CA}', Bin),
        "leftthreetimes" => ('\u{22CB}', Bin),
        "rightthreetimes" => ('\u{22CC}', Bin),
        "divideontimes" => ('\u{22C7}', Bin),
        "dotplus" => ('\u{2214}', Bin),
        "smallsetminus" => ('\u{2216}', Bin),
        "barwedge" => ('\u{22BC}', Bin),
        "veebar" => ('\u{22BB}', Bin),
        // U+2A5E is the proper codepoint, but Latin Modern draws the same shape only as ⌆.
        "doublebarwedge" => ('\u{2306}', Bin),
        "curlywedge" => ('\u{22CF}', Bin),
        "curlyvee" => ('\u{22CE}', Bin),
        "intercal" => ('\u{22BA}', Bin),
        "Cup" | "doublecup" => ('\u{22D3}', Bin),
        "Cap" | "doublecap" => ('\u{22D2}', Bin),

        // Relations
        "leq" | "le" => ('\u{2264}', Rel),
        "geq" | "ge" => ('\u{2265}', Rel),
        "lt" => ('<', Rel),
        "gt" => ('>', Rel),
        "ll" => ('\u{226A}', Rel),
        "gg" => ('\u{226B}', Rel),
        "neq" | "ne" => ('\u{2260}', Rel),
        "approx" | "thickapprox" => ('\u{2248}', Rel),
        "asymp" => ('\u{224D}', Rel),
        "sim" | "thicksim" => ('\u{223C}', Rel),
        "simeq" => ('\u{2243}', Rel),
        "cong" => ('\u{2245}', Rel),
        "equiv" => ('\u{2261}', Rel),
        "propto" | "varpropto" => ('\u{221D}', Rel),
        "subset" => ('\u{2282}', Rel),
        "supset" => ('\u{2283}', Rel),
        "subseteq" => ('\u{2286}', Rel),
        "supseteq" => ('\u{2287}', Rel),
        "nsubseteq" => ('\u{2288}', Rel),
        "in" => ('\u{2208}', Rel),
        "notin" => ('\u{2209}', Rel),
        "ni" => ('\u{220B}', Rel),
        "backepsilon" => ('\u{220D}', Rel),
        "perp" => ('\u{22A5}', Rel),
        "parallel" | "shortparallel" => ('\u{2225}', Rel),
        "mid" | "shortmid" => ('\u{2223}', Rel),
        "models" | "vDash" => ('\u{22A8}', Rel),
        "vdash" => ('\u{22A2}', Rel),
        "colon" => (':', AtomClass::Punct),

        "prec" => ('\u{227A}', Rel),
        "succ" => ('\u{227B}', Rel),
        "preceq" => ('\u{2AAF}', Rel),
        "succeq" => ('\u{2AB0}', Rel),
        "precsim" => ('\u{227E}', Rel),
        "succsim" => ('\u{227F}', Rel),
        "nprec" => ('\u{2280}', Rel),
        "nsucc" => ('\u{2281}', Rel),
        "lll" => ('\u{22D8}', Rel),
        "ggg" => ('\u{22D9}', Rel),
        "lessgtr" => ('\u{2276}', Rel),
        "gtrless" => ('\u{2277}', Rel),
        "lesssim" => ('\u{2272}', Rel),
        "gtrsim" => ('\u{2273}', Rel),
        "leqslant" => ('\u{2A7D}', Rel),
        "geqslant" => ('\u{2A7E}', Rel),
        "lneq" => ('\u{2A87}', Rel),
        "gneq" => ('\u{2A88}', Rel),
        "nleq" => ('\u{2270}', Rel),
        "ngeq" => ('\u{2271}', Rel),
        "nless" => ('\u{226E}', Rel),
        "ngtr" => ('\u{226F}', Rel),
        "leqq" => ('\u{2266}', Rel),
        "geqq" => ('\u{2267}', Rel),
        "lneqq" | "lvertneqq" => ('\u{2268}', Rel),
        "gneqq" | "gvertneqq" => ('\u{2269}', Rel),
        "lnsim" => ('\u{22E6}', Rel),
        "gnsim" => ('\u{22E7}', Rel),
        "lnapprox" => ('\u{2A89}', Rel),
        "gnapprox" => ('\u{2A8A}', Rel),
        "lessdot" => ('\u{22D6}', Rel),
        "gtrdot" => ('\u{22D7}', Rel),
        "lesseqgtr" => ('\u{22DA}', Rel),
        "gtreqless" => ('\u{22DB}', Rel),
        "lesseqqgtr" => ('\u{2A8B}', Rel),
        "gtreqqless" => ('\u{2A8C}', Rel),
        "eqslantless" => ('\u{2A95}', Rel),
        "eqslantgtr" => ('\u{2A96}', Rel),
        "lessapprox" => ('\u{2A85}', Rel),
        "gtrapprox" => ('\u{2A86}', Rel),
        "preccurlyeq" => ('\u{227C}', Rel),
        "succcurlyeq" => ('\u{227D}', Rel),
        "curlyeqprec" => ('\u{22DE}', Rel),
        "curlyeqsucc" => ('\u{22DF}', Rel),
        "npreceq" => ('\u{22E0}', Rel),
        "nsucceq" => ('\u{22E1}', Rel),
        "precnsim" => ('\u{22E8}', Rel),
        "succnsim" => ('\u{22E9}', Rel),

        "subsetneq" | "varsubsetneq" => ('\u{228A}', Rel),
        "supsetneq" | "varsupsetneq" => ('\u{228B}', Rel),
        "Subset" => ('\u{22D0}', Rel),
        "Supset" => ('\u{22D1}', Rel),
        "nsubset" => ('\u{2284}', Rel),
        "nsupset" => ('\u{2285}', Rel),
        "nsupseteq" => ('\u{2289}', Rel),
        "subseteqq" => ('\u{2AC5}', Rel),
        "supseteqq" => ('\u{2AC6}', Rel),
        "sqsubset" => ('\u{228F}', Rel),
        "sqsupset" => ('\u{2290}', Rel),
        "sqsubseteq" => ('\u{2291}', Rel),
        "sqsupseteq" => ('\u{2292}', Rel),
        "owns" => ('\u{220B}', Rel),
        "notni" => ('\u{220C}', Rel),

        "doteq" => ('\u{2250}', Rel),
        "risingdotseq" => ('\u{2253}', Rel),
        "fallingdotseq" => ('\u{2252}', Rel),
        "triangleq" => ('\u{225C}', Rel),
        "approxeq" => ('\u{224A}', Rel),
        "backsim" => ('\u{223D}', Rel),
        "ncong" => ('\u{2247}', Rel),
        "nsim" => ('\u{2241}', Rel),
        "coloneqq" | "colonequals" => ('\u{2254}', Rel),
        "vcentcolon" | "ratio" => ('\u{2236}', Rel),
        "dblcolon" => ('\u{2237}', Rel),
        "eqcirc" => ('\u{2256}', Rel),
        "circeq" => ('\u{2257}', Rel),
        "bumpeq" => ('\u{224F}', Rel),
        "Bumpeq" => ('\u{224E}', Rel),
        "eqsim" => ('\u{2242}', Rel),
        "doteqdot" | "Doteq" => ('\u{2251}', Rel),
        "eqqcolon" => ('\u{2255}', Rel),

        "Vdash" => ('\u{22A9}', Rel),
        "Vvdash" => ('\u{22AA}', Rel),
        "dashv" => ('\u{22A3}', Rel),
        "nvdash" => ('\u{22AC}', Rel),
        "nvDash" => ('\u{22AD}', Rel),
        "nVdash" => ('\u{22AE}', Rel),
        "nVDash" => ('\u{22AF}', Rel),
        "origof" => ('\u{22B6}', Rel),
        "imageof" => ('\u{22B7}', Rel),
        "multimap" => ('\u{22B8}', Rel),

        "vartriangleleft" => ('\u{22B2}', Rel),
        "vartriangleright" => ('\u{22B3}', Rel),
        "trianglelefteq" => ('\u{22B4}', Rel),
        "trianglerighteq" => ('\u{22B5}', Rel),
        "ntriangleleft" => ('\u{22EA}', Rel),
        "ntriangleright" => ('\u{22EB}', Rel),
        "ntrianglelefteq" => ('\u{22EC}', Rel),
        "ntrianglerighteq" => ('\u{22ED}', Rel),
        "vartriangle" => ('\u{25B3}', Rel),
        "bowtie" | "Join" => ('\u{22C8}', Rel),
        "smile" | "smallsmile" => ('\u{2323}', Rel),
        "frown" | "smallfrown" => ('\u{2322}', Rel),
        "between" => ('\u{226C}', Rel),
        "pitchfork" => ('\u{22D4}', Rel),
        "nmid" | "nshortmid" => ('\u{2224}', Rel),
        "nparallel" | "nshortparallel" => ('\u{2226}', Rel),

        // Arrows (all relations)
        "to" | "rightarrow" => ('\u{2192}', Rel),
        "leftarrow" | "gets" => ('\u{2190}', Rel),
        "leftrightarrow" => ('\u{2194}', Rel),
        "Rightarrow" => ('\u{21D2}', Rel),
        "Leftarrow" => ('\u{21D0}', Rel),
        "Leftrightarrow" => ('\u{21D4}', Rel),
        "mapsto" => ('\u{21A6}', Rel),
        "hookrightarrow" => ('\u{21AA}', Rel),
        "hookleftarrow" => ('\u{21A9}', Rel),
        "uparrow" => ('\u{2191}', Rel),
        "downarrow" => ('\u{2193}', Rel),
        "updownarrow" => ('\u{2195}', Rel),
        "Uparrow" => ('\u{21D1}', Rel),
        "Downarrow" => ('\u{21D3}', Rel),
        "Updownarrow" => ('\u{21D5}', Rel),
        "nearrow" => ('\u{2197}', Rel),
        "searrow" => ('\u{2198}', Rel),
        "nwarrow" => ('\u{2196}', Rel),
        "swarrow" => ('\u{2199}', Rel),

        // amsmath's `\implies` and `\iff` are the long arrows.
        "longrightarrow" => ('\u{27F6}', Rel),
        "longleftarrow" => ('\u{27F5}', Rel),
        "longleftrightarrow" => ('\u{27F7}', Rel),
        "Longleftarrow" | "impliedby" => ('\u{27F8}', Rel),
        "Longrightarrow" | "implies" => ('\u{27F9}', Rel),
        "Longleftrightarrow" | "iff" => ('\u{27FA}', Rel),
        "longmapsto" => ('\u{27FC}', Rel),

        "rightharpoonup" => ('\u{21C0}', Rel),
        "rightharpoondown" => ('\u{21C1}', Rel),
        "leftharpoonup" => ('\u{21BC}', Rel),
        "leftharpoondown" => ('\u{21BD}', Rel),
        "rightleftharpoons" => ('\u{21CC}', Rel),
        "leftrightharpoons" => ('\u{21CB}', Rel),
        "upharpoonright" | "restriction" => ('\u{21BE}', Rel),
        "upharpoonleft" => ('\u{21BF}', Rel),
        "downharpoonright" => ('\u{21C2}', Rel),
        "downharpoonleft" => ('\u{21C3}', Rel),

        "twoheadrightarrow" => ('\u{21A0}', Rel),
        "twoheadleftarrow" => ('\u{219E}', Rel),
        "rightarrowtail" => ('\u{21A3}', Rel),
        "leftarrowtail" => ('\u{21A2}', Rel),
        "looparrowright" => ('\u{21AC}', Rel),
        "looparrowleft" => ('\u{21AB}', Rel),
        "rightrightarrows" => ('\u{21C9}', Rel),
        "leftleftarrows" => ('\u{21C7}', Rel),
        "rightleftarrows" => ('\u{21C4}', Rel),
        "leftrightarrows" => ('\u{21C6}', Rel),
        "upuparrows" => ('\u{21C8}', Rel),
        "downdownarrows" => ('\u{21CA}', Rel),
        "circlearrowleft" => ('\u{21BA}', Rel),
        "circlearrowright" => ('\u{21BB}', Rel),
        "curvearrowleft" => ('\u{21B6}', Rel),
        "curvearrowright" => ('\u{21B7}', Rel),
        "Lleftarrow" => ('\u{21DA}', Rel),
        "Rrightarrow" => ('\u{21DB}', Rel),
        "Lsh" => ('\u{21B0}', Rel),
        "Rsh" => ('\u{21B1}', Rel),
        "leadsto" | "rightsquigarrow" => ('\u{21DD}', Rel),
        "leftrightsquigarrow" => ('\u{21AD}', Rel),

        "nrightarrow" => ('\u{219B}', Rel),
        "nleftarrow" => ('\u{219A}', Rel),
        "nleftrightarrow" => ('\u{21AE}', Rel),
        "nRightarrow" => ('\u{21CF}', Rel),
        "nLeftarrow" => ('\u{21CD}', Rel),
        "nLeftrightarrow" => ('\u{21CE}', Rel),

        // Named constants and sets
        "infty" => ('\u{221E}', Ord),
        "partial" => ('\u{2202}', Ord),
        "nabla" => ('\u{2207}', Ord),
        "forall" => ('\u{2200}', Ord),
        "exists" => ('\u{2203}', Ord),
        "nexists" => ('\u{2204}', Ord),
        "emptyset" | "varnothing" => ('\u{2205}', Ord),
        "top" => ('\u{22A4}', Ord),
        "bot" => ('\u{22A5}', Ord),
        "angle" => ('\u{2220}', Ord),
        "degree" | "textdegree" => ('\u{00B0}', Ord),
        "hbar" | "hslash" => ('\u{210F}', Ord),
        "ell" => ('\u{2113}', Ord),
        "Re" => ('\u{211C}', Ord),
        "Im" => ('\u{2111}', Ord),
        "wp" => ('\u{2118}', Ord),
        "aleph" => ('\u{2135}', Ord),
        "beth" => ('\u{2136}', Ord),
        "gimel" => ('\u{2137}', Ord),
        "daleth" => ('\u{2138}', Ord),
        "prime" => ('\u{2032}', Ord),
        "dprime" => ('\u{2033}', Ord),
        "trprime" => ('\u{2034}', Ord),
        "backprime" => ('\u{2035}', Ord),
        "therefore" => ('\u{2234}', Ord),
        "because" => ('\u{2235}', Ord),
        "complement" => ('\u{2201}', Ord),
        "measuredangle" => ('\u{2221}', Ord),
        "sphericalangle" => ('\u{2222}', Ord),
        "mho" => ('\u{2127}', Ord),
        "Finv" => ('\u{2132}', Ord),
        "Game" => ('\u{2141}', Ord),
        "circledS" => ('\u{24C8}', Ord),
        "eth" => ('\u{00F0}', Ord),
        "imath" => ('\u{0131}', Ord),
        "jmath" => ('\u{0237}', Ord),
        "Bbbk" => ('\u{1D55C}', Ord),
        "diagup" => ('\u{2215}', Ord),
        "diagdown" => ('\u{2216}', Ord),
        "surd" => ('\u{221A}', Ord),
        // An integral sign that never grows.
        "smallint" => ('\u{222B}', Op),

        // Shapes
        "Box" | "square" => ('\u{25A1}', Ord),
        "blacksquare" => ('\u{25A0}', Ord),
        "triangle" => ('\u{25B3}', Ord),
        "triangledown" => ('\u{25BD}', Ord),
        "blacktriangle" => ('\u{25B2}', Ord),
        "blacktriangledown" => ('\u{25BC}', Ord),
        "blacktriangleleft" => ('\u{25C0}', Ord),
        "blacktriangleright" => ('\u{25B6}', Ord),
        // KaTeX's `\Diamond` is the lozenge too; Latin Modern has no ◇.
        "lozenge" | "Diamond" => ('\u{25CA}', Ord),
        "blacklozenge" => ('\u{29EB}', Ord),
        "bigstar" => ('\u{2605}', Ord),
        "checkmark" => ('\u{2713}', Ord),
        "maltese" => ('\u{2720}', Ord),

        // Cards, music and the printer's marks
        "clubsuit" => ('\u{2663}', Ord),
        "diamondsuit" => ('\u{2662}', Ord),
        "heartsuit" => ('\u{2661}', Ord),
        "spadesuit" => ('\u{2660}', Ord),
        "flat" => ('\u{266D}', Ord),
        "natural" => ('\u{266E}', Ord),
        "sharp" => ('\u{266F}', Ord),
        "S" => ('\u{00A7}', Ord),
        "P" => ('\u{00B6}', Ord),
        "pounds" => ('\u{00A3}', Ord),
        "yen" => ('\u{00A5}', Ord),
        "copyright" => ('\u{00A9}', Ord),
        "circledR" => ('\u{00AE}', Ord),
        "euro" => ('\u{20AC}', Ord),
        "cent" => ('\u{00A2}', Ord),
        "textbackslash" => ('\u{005C}', Ord),
        "textasciitilde" => ('~', Ord),
        "textunderscore" => ('_', Ord),

        // Letters from text mode
        "AA" => ('\u{00C5}', Ord),
        "aa" => ('\u{00E5}', Ord),
        "AE" => ('\u{00C6}', Ord),
        "ae" => ('\u{00E6}', Ord),
        "OE" => ('\u{0152}', Ord),
        "oe" => ('\u{0153}', Ord),
        "O" => ('\u{00D8}', Ord),
        "o" => ('\u{00F8}', Ord),
        "ss" => ('\u{00DF}', Ord),
        "i" => ('\u{0131}', Ord),
        "j" => ('\u{0237}', Ord),

        // Ellipses
        // `\mathinner` in TeX.
        "ldots" | "dots" | "dotsc" | "dotso" | "mathellipsis" => ('\u{2026}', Inner),
        "cdots" | "dotsb" | "dotsm" | "dotsi" => ('\u{22EF}', Inner),
        "vdots" => ('\u{22EE}', Inner),
        "ddots" => ('\u{22F1}', Inner),
        "iddots" => ('\u{22F0}', Inner),

        // Delimiters, usable bare as well as after \left / \right
        "lfloor" => ('\u{230A}', Open),
        "rfloor" => ('\u{230B}', Close),
        "lceil" => ('\u{2308}', Open),
        "rceil" => ('\u{2309}', Close),
        "langle" => ('\u{27E8}', Open),
        "rangle" => ('\u{27E9}', Close),
        "lvert" | "vert" => ('|', Ord),
        "rvert" => ('|', Ord),
        "Vert" | "|" => ('\u{2016}', Ord),
        "backslash" => ('\u{005C}', Ord),
        // `{` and `}` are grouping to the tokenizer, so literal braces need names.
        "lbrace" => ('{', Open),
        "rbrace" => ('}', Close),
        "lbrack" => ('[', Open),
        "rbrack" => (']', Close),
        "lparen" => ('(', Open),
        "rparen" => (')', Close),
        "lVert" => ('\u{2016}', Open),
        "rVert" => ('\u{2016}', Close),
        "llbracket" => ('\u{27E6}', Open),
        "rrbracket" => ('\u{27E7}', Close),
        "lgroup" => ('\u{27EE}', Open),
        "rgroup" => ('\u{27EF}', Close),
        "lmoustache" => ('\u{23B0}', Open),
        "rmoustache" => ('\u{23B1}', Close),
        "ulcorner" => ('\u{250C}', Open),
        "urcorner" => ('\u{2510}', Close),
        "llcorner" => ('\u{2514}', Open),
        "lrcorner" => ('\u{2518}', Close),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_commands_are_rejected() {
        assert_eq!(resolve("thereisnosuchcommand"), None);
        assert_eq!(resolve(""), None);
    }

    #[test]
    fn epsilon_and_phi_variants_are_not_swapped() {
        assert_eq!(resolve("epsilon").map(|s| s.0), Some('\u{03F5}'));
        assert_eq!(resolve("varepsilon").map(|s| s.0), Some('\u{03B5}'));
        assert_eq!(resolve("phi").map(|s| s.0), Some('\u{03D5}'));
        assert_eq!(resolve("varphi").map(|s| s.0), Some('\u{03C6}'));
    }

    #[test]
    fn classes_match_tex() {
        assert_eq!(resolve("cap").map(|s| s.1), Some(Bin));
        assert_eq!(resolve("subset").map(|s| s.1), Some(Rel));
        assert_eq!(resolve("to").map(|s| s.1), Some(Rel));
        assert_eq!(resolve("infty").map(|s| s.1), Some(Ord));
        assert_eq!(resolve("lfloor").map(|s| s.1), Some(Open));
        assert_eq!(resolve("rfloor").map(|s| s.1), Some(Close));
        assert_eq!(resolve("ldots").map(|s| s.1), Some(Inner));
        assert_eq!(resolve("neg").map(|s| s.1), Some(Ord));
    }

    #[test]
    fn single_and_double_bars_are_distinct() {
        assert_eq!(resolve("vert").map(|s| s.0), Some('|'));
        assert_eq!(resolve("Vert").map(|s| s.0), Some('\u{2016}'));
        assert_eq!(resolve("|").map(|s| s.0), Some('\u{2016}'));
    }
}
