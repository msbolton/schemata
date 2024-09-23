grammar Schemata;

// Parser rules

schemata
    : (statement)+ EOF
    ;

statement
    : namespaceDeclaration
    | importDeclaration
    | schemaDeclaration
    | enumDeclaration
    | NEWLINE+
    ;

namespaceDeclaration
    : 'namespace' qualifiedName NEWLINE+
    ;

importDeclaration
    : 'import' STRING_LITERAL NEWLINE+
    ;

schemaDeclaration
    : 'schema' ID '{' schemaBody '}' NEWLINE*
    ;

schemaBody
    : (schemaFieldDeclaration)+
    ;

schemaFieldDeclaration
    : annotations? ID typeExpression fieldAttributes? ('=' expression)? NEWLINE+
    ;

fieldAttributes
    : fieldAttribute+
    ;

fieldAttribute
    : '@' ID ('(' attributeParameters? ')')?
    ;

attributeParameters
    : attributeParameter (',' attributeParameter)*
    ;

attributeParameter
    : ID '=' literal
    ;

typeExpression
    : basicType nullable?
    | customType nullable?
    | collectionType nullable?
    ;

collectionType
    : basicType '[' ']'   // Collections of basic types
    | customType '[' ']'; // Collections of custom types

nullable
    : '?'
    ;

basicType
    : 'int'
    | 'string'
    | 'boolean'
    | 'float'
    | 'double'
    | 'datetime'
    | 'date'
    ;

customType
    : ID
    ;

enumDeclaration
    : 'enum' ID '{' enumBody '}' NEWLINE*
    ;

enumBody
    : (enumValue (NEWLINE | ',' | NEWLINE enumValue)*)? enumValue? NEWLINE*
    ;

enumValue
    : ID
    ;

annotations
    : annotation+
    ;

annotation
    : '@' ID ('(' annotationParameters? ')')?
    ;

annotationParameters
    : annotationParameter (',' annotationParameter)*
    ;

annotationParameter
    : ID '=' literal
    ;

expression
    : literal
    ;

literal
    : STRING_LITERAL
    | NUMBER_LITERAL
    | 'true'
    | 'false'
    ;

qualifiedName
    : ID ('.' ID)*
    ;

// Lexer rules

ID
    : [a-zA-Z_][a-zA-Z_0-9]*
    ;

STRING_LITERAL
    : '"' ( ~["\\\r\n] | '\\' . )* '"'
    ;

NUMBER_LITERAL
    : [0-9]+ ('.' [0-9]+)?
    ;

NEWLINE
    : ( '\r'? '\n' )+
    ;

WS
    : [ \t]+ -> skip
    ;

COMMENT
    : '//' ~[\r\n]* -> skip
    ;

MULTILINE_COMMENT
    : '/*' .*? '*/' -> skip
    ;
