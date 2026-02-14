module.exports = grammar({
  name: "gpc",

  extras: ($) => [/\s/, $.line_comment, $.block_comment],

  word: ($) => $.identifier,

  conflicts: ($) => [[$._statement, $.expression]],

  rules: {
    source_file: ($) => repeat($._global_declaration),

    _global_declaration: ($) =>
      choice(
        $.main_block,
        $.init_block,
        $.function_declaration,
        $.combo_declaration,
        $.variable_declaration,
        $.const_variable_declaration,
        $.define_declaration,
        $.enum_declaration,
        $.data_declaration,
        $.remap_declaration,
        $.unmap_declaration,

        // Experimental features
        $.macro_declaration,
        $.macro_call,
        $.import,
      ),

    // import foo/bar; (infer .gpc ending if not present)
    // import "foo/bar.gpc";
    import: ($) =>
      seq(
        "import",
        field(
          "path",
          choice(
            $.import_identifier,
            seq('"', $.import_identifier, optional(".gpc"), '"'),
          ),
        ),
        ";",
      ),

    // define! NAME (<PARAMS>) { ... }
    macro_declaration: ($) =>
      seq(
        "define!",
        field("name", $.identifier),
        "(",
        optional($.parameter_list),
        ")",
        field("body", $.block),
      ),

    // NAME!(ARGS) or NAME!(ARGS) { ...}
    macro_call: ($) =>
      prec.right(
        15,
        seq(
          field("name", $.identifier),
          "(",
          optional($.argument_list),
          ")",
          "!",
          optional(
            field(
              "body",
              seq(
                "{",
                optional(choice(repeat1($._statement), $.expression)),
                "}",
              ),
            ),
          ),
        ),
      ),

    // Macro body can contain statements or just an expression
    macro_body: ($) =>
      seq("{", optional(choice(repeat1($._statement), $.expression)), "}"),

    remap_declaration: ($) =>
      seq(
        "remap",
        field("from", $.identifier),
        "->",
        field("to", $.identifier),
        ";",
      ),

    unmap_declaration: ($) => seq("unmap", field("button", $.identifier), ";"),

    // Special blocks
    main_block: ($) => seq("main", optional(seq("(", ")")), $.block),

    init_block: ($) => seq("init", optional(seq("(", ")")), $.block),

    // Function declarations
    function_declaration: ($) =>
      seq(
        "function",
        field("name", $.identifier),
        "(",
        optional($.parameter_list),
        ")",
        field("body", $.block),
      ),

    parameter_list: ($) =>
      seq(
        optional("int"),
        $.identifier,
        repeat(seq(",", optional("int"), $.identifier)),
      ),

    // Combo declarations
    combo_declaration: ($) =>
      seq(
        choice("combo", "fcombo"),
        field("name", $.identifier),
        optional(seq("(", ")")),
        field("body", $.block),
      ),

    // Variable declarations
    variable_declaration: ($) =>
      seq(
        field("type", $.type),
        $.variable_declarator,
        repeat(seq(",", $.variable_declarator)),
        ";",
      ),

    const_variable_declaration: ($) =>
      seq(
        "const",
        field("type", $.type),
        $.variable_declarator,
        repeat(seq(",", $.variable_declarator)),
        ";",
      ),

    variable_declarator: ($) =>
      seq(
        field("name", $.identifier),
        repeat($.array_dimension),
        optional(seq("=", field("value", $._initializer))),
      ),

    array_dimension: ($) =>
      choice(
        seq("[", "]"),
        seq("[", choice($.integer_literal, $.identifier), "]"),
      ),

    _initializer: ($) => choice($.expression, $.array_initializer),

    array_initializer: ($) =>
      seq(
        "{",
        optional(seq($._initializer, repeat(seq(",", $._initializer)))),
        "}",
      ),

    // Define declarations
    define_declaration: ($) =>
      seq(
        "define",
        field("name", $.identifier),
        "=",
        field("value", $.expression),
        ";",
      ),

    // Enum declarations
    enum_declaration: ($) =>
      seq("enum", "{", optional($.enum_variant_list), "}"),

    enum_variant_list: ($) =>
      seq($.enum_variant, repeat(seq(",", $.enum_variant)), optional(",")),

    enum_variant: ($) =>
      seq(
        field("name", $.identifier),
        optional(
          seq("=", field("value", choice($.integer_literal, $.identifier))),
        ),
      ),

    // Data declarations
    data_declaration: ($) =>
      seq("data", "(", optional($.expression_list), ")", ";"),

    // Statements
    _statement: ($) =>
      choice(
        $.block,
        $.macro_call,
        $.call_statement,
        $.expression_statement,
        $.if_statement,
        $.while_statement,
        $.do_while_statement,
        $.for_statement,
        $.switch_statement,
        $.return_statement,
        $.break_statement,
        $.continue_statement,
        $.assignment_statement,
        $.placeholder,
        ";", // empty statement
      ),

    block: ($) => seq("{", repeat($._statement), "}"),

    expression_statement: ($) => seq($.expression, ";"),

    call_statement: ($) =>
      seq(
        field("function", $.identifier),
        "(",
        optional($.argument_list),
        ")",
        ";",
      ),

    placeholder: ($) => "%0",

    assignment_statement: ($) =>
      seq(
        field("left", $.expression),
        field("operator", choice("=", "+=", "-=", "*=", "/=", "<<=", ">>=")),
        field("right", $.expression),
        ";",
      ),

    if_statement: ($) =>
      prec.right(
        seq(
          "if",
          "(",
          field("condition", $.expression),
          ")",
          field("consequence", $._statement),
          optional(seq("else", field("alternative", $._statement))),
        ),
      ),

    while_statement: ($) =>
      seq(
        "while",
        "(",
        field("condition", $.expression),
        ")",
        field("body", $._statement),
      ),

    do_while_statement: ($) =>
      seq(
        "do",
        field("body", $._statement),
        "while",
        "(",
        field("condition", $.expression),
        ")",
        ";",
      ),

    for_statement: ($) =>
      seq(
        "for",
        "(",
        field(
          "init",
          optional(choice($.assignment_statement_no_semi, $.expression)),
        ),
        ";",
        field("condition", optional($.expression)),
        ";",
        field(
          "update",
          optional(choice($.assignment_statement_no_semi, $.expression)),
        ),
        ")",
        field("body", $._statement),
      ),

    assignment_statement_no_semi: ($) =>
      seq(
        field("left", $.expression),
        field("operator", choice("=", "+=", "-=", "*=", "/=", "<<=", ">>=")),
        field("right", $.expression),
      ),

    switch_statement: ($) =>
      seq(
        "switch",
        "(",
        field("value", $.expression),
        ")",
        "{",
        repeat(choice($.case_clause, $.default_clause)),
        "}",
      ),

    case_clause: ($) =>
      seq("case", $.expression, repeat(seq(",", $.expression)), ":", $.block),

    default_clause: ($) => seq("default", ":", $.block),

    return_statement: ($) => seq("return", optional($.expression), ";"),

    break_statement: ($) => seq("break", ";"),

    continue_statement: ($) => seq("continue", ";"),

    // Expressions
    expression: ($) =>
      choice(
        $.binary_expression,
        $.unary_expression,
        $.postfix_expression,
        $.call_expression,
        $.macro_call,
        $.array_access,
        $._primary_expression,
      ),

    _primary_expression: ($) =>
      prec(
        -1,
        choice(
          $.identifier,
          $.integer_literal,
          $.string_literal,
          $.parenthesized_expression,
        ),
      ),

    expression_list: ($) => seq($.expression, repeat(seq(",", $.expression))),

    parenthesized_expression: ($) => seq("(", $.expression, ")"),

    binary_expression: ($) => {
      const table = [
        [prec.left, 1, "||"],
        [prec.left, 2, "^^"],
        [prec.left, 3, "&&"],
        [prec.left, 4, "|"],
        [prec.left, 5, "^"],
        [prec.left, 6, "&"],
        [prec.left, 7, choice("==", "!=")],
        [prec.left, 8, choice("<", "<=", ">", ">=")],
        [prec.left, 9, choice("<<", ">>")],
        [prec.left, 10, choice("+", "-")],
        [prec.left, 11, choice("*", "/", "%")],
      ];

      return choice(
        ...table.map(([fn, precedence, operator]) =>
          fn(
            precedence,
            seq(
              field("left", $.expression),
              field("operator", operator),
              field("right", $.expression),
            ),
          ),
        ),
      );
    },

    unary_expression: ($) =>
      prec.right(
        12,
        seq(
          field("operator", choice("-", "!", "~", "++", "--")),
          field("operand", $.expression),
        ),
      ),

    postfix_expression: ($) =>
      prec(
        13,
        seq(
          field("operand", $.expression),
          field("operator", choice("++", "--")),
        ),
      ),

    call_expression: ($) =>
      prec(
        14,
        seq(
          field("function", $.expression),
          "(",
          optional($.argument_list),
          ")",
        ),
      ),

    argument_list: ($) => seq($.expression, repeat(seq(",", $.expression))),

    array_access: ($) =>
      prec(
        14,
        seq(
          field("array", $.expression),
          "[",
          field("index", $.expression),
          "]",
        ),
      ),

    // Types
    type: ($) =>
      choice(
        "int",
        "int8",
        "int16",
        "uint8",
        "uint16",
        "byte",
        "char",
        "string",
        "image",
        "ps5adt",
      ),

    // Literals and identifiers
    identifier: ($) => /[a-zA-Z_][a-zA-Z0-9_]*/,
    import_identifier: ($) => /[a-zA-Z0-9_\/]+/,

    integer_literal: ($) => choice(/0[xX][0-9a-fA-F]+/, /[0-9]+/),

    string_literal: ($) => /"([^"\\]|\\.)*"/,

    // Comments
    line_comment: ($) => token(seq("//", /.*/)),

    block_comment: ($) => token(seq("/*", /[^*]*\*+([^/*][^*]*\*+)*/, "/")),
  },
});
