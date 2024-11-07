# A backend for analyzing text #

The backend performs the following operations for provided text input:
- attempts to identify input text encoding and decode it before performing any
  further steps, if provided text is not valid UTF-8
- attempts to identify programming language
- attempts to identify natural (human-spoken) language
- extracts URIs found in text
- signals if there is a sequence of characters which looks like a phone number
- signals if there is a sequence of characters which looks like a credit card
  number with a valid Luhn checksum, and
- provides total number of characters, number of ASCII characters, number of
  digits characters and number of whitespace characters in text.

## Programming language identification ##
Programming language identification is performed with `guesslang` machine
learning model, and then most probable guessed languages are verified with
`tree-sitter`. The first language which passes through `tree-sitter` grammar
verification "wins".

The set of languages supported by `guesslang` ML model restricted to a much
smaller set.

Excluded languages are:
- compiled languages (because at the moment there is no any value in detecting
  these languages), and
- languages for which there is no `tree-sitter` grammar definition available
  (what makes it impossible to perform basic grammar verification to confirm or
  reject machine learning model prediction).

## Human language identification ##
Natural language identification is performed by means of `lingua` crate. It
relies on statistical probabilities of different n-grams (n-letter sequences)
to appear in different human-spoken languages to perform language
identification task.

## Build notes ##
This worker requires the [TensorFlow C library](https://www.tensorflow.org/install/lang_c)
aka libtensorflow.

You may be able to find prebuild binaries for your architecture or you may need to compile
them on your own.
Check the above link for info on both cases.
