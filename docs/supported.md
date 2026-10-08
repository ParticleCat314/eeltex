# Supported LaTeX

| | |
| --- | --- |
| Symbols | About 480 control sequences from base LaTeX and `amssymb`, plus literal Unicode maths |
| Scripts | Sub- and superscripts, primes |
| Greek | Lower and upper case, the Latin-looking capitals (`\Alpha` …), italic capitals `\varGamma` … `\varOmega` |
| Fractions | `\frac` `\dfrac` `\tfrac` `\cfrac` `\genfrac`, `\binom` `\dbinom` `\tbinom`, `\over` `\atop` `\choose` `\brace` `\brack` |
| Radicals | `\sqrt`, with an optional index |
| Delimiters | Stretchy `\left…\middle…\right`, including the vertical arrows; `\big` through `\Bigg` |
| Big operators | `\sum` `\prod` `\int` `\oint` `\ointclockwise` `\bigoplus` `\bigvee` and the rest, `\intop` `\smallint`, with `\limits` / `\nolimits` |
| Function names | Upright, including the limit-taking `\lim` `\limsup` `\varliminf` `\varinjlim` `\max` `\Pr`, and `\sh` `\tg` `\ctg` `\cosec` … |
| Modular arithmetic | `\bmod` `\pmod` `\pod` `\mod` |
| Alphabets | `\mathbb` `\mathcal` `\mathscr` `\mathfrak` `\mathbf` `\mathbfit` `\mathsfit` `\boldsymbol` `\pmb` and friends |
| Prose | `\text` `\textbf` `\textit` `\textsf` `\texttt` `\textrm` `\emph` `\mbox` |
| Styles | `\displaystyle` `\textstyle` `\scriptstyle` `\scriptscriptstyle` |
| Sizes ¹ | `\tiny` `\scriptsize` `\footnotesize` `\small` `\normalsize` `\large` `\Large` `\LARGE` `\huge` `\Huge` |
| Accents | `\hat` `\widehat` `\widecheck` `\bar` `\vec` `\acute` `\grave` `\breve` `\check` `\mathring`, `\dot` through `\ddddot`, `\utilde`, the text accents `\'` `\"` `\~` `\u` `\v` `\H` `\r` … |
| Over and under | `\overline` `\underline` `\overlinesegment` `\overset` `\underset` `\stackrel` `\substack` |
| Braces and arrows | `\overbrace` `\underbrace` `\overbracket` `\overgroup` `\overrightarrow` `\Overrightarrow` `\overleftharpoon` `\underleftrightarrow` … |
| Extensible arrows | `\xrightarrow[under]{over}`, `\xrightleftharpoons` `\xlongequal` `\xtofrom` and the rest |
| Environments | `aligned` `gather` `multline`, the `matrix` family and its starred forms, `cases` `dcases` `rcases`, `array` and `darray` with `l c r` columns, `subarray`; ¹ `\hline` `\hdashline` and `\| :` column rules |
| Spacing | Explicit spaces, `~`, `\enspace`, `\hspace` `\kern` `\mkern` `\mskip` `\hskip` in any TeX unit |
| Boxes ¹ | `\boxed` `\fbox` `\colorbox` `\fcolorbox`, `\cancel` `\bcancel` `\xcancel` `\cancelto` `\sout` |
| Struts and laps ¹ | `\rule` `\strut` `\mathstrut` `\smash` `\llap` `\rlap` `\clap` `\raisebox` |
| The rest | `\not` · `\phantom` `\hphantom` `\vphantom` · the `\mathXXX` class overrides · `\color` and `\textcolor` |

¹ With the `formatting` feature; without it these are parse errors that say so.
`\mathstrut` needs no feature.

Spacing follows TeX's atom classes and inter-atom spacing table, including
binary-to-ordinary demotion (`-x` is a sign).

Anything else is a **parse error**, never a guess. `Error::Parse` carries the
byte offset so a live preview can underline it. Not supported:

- Text and document mode, `\newcommand`, `\def`
- `xcolor` blends and colour models (`\color` takes a name, a hex triple or `rgb(…)`)
- `p{…}` columns, `CD`, `\sideset`, `\prescript`
- Line breaking, numbering, `\tag`, `\label`
- mhchem, `physics`, `braket`, `\href` and the HTML extensions
- RTL, vertical writing, variable fonts

**The embedded New Computer Modern Math has no glyph for `\circledS`; it parses, and draws as
the missing-glyph box unless the font you supply has it.**
