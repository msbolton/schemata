# Schemata Grammar Overview
## Introduction
The Schemata grammar defines an Interface Definition Language (IDL) used for defining schemas that can represent entities, data models, and mappings for various programming languages and databases. It allows for the creation of schemas with fields, types (both basic and custom), and relationships between entities. This document describes the structure and components of the grammar used to parse `.schema` files.

## Grammar Structure
### Namespace Declaration
The namespace defines a scope for organizing schema definitions, similar to packages in Java or namespaces in C#.

#### Example
```plaintext
namespace com.example.models
```

#### Grammar Rule
```antlrv4-tool
namespaceDeclaration
    : 'namespace' qualifiedName NEWLINE+
    ;
```

### Import Declaration
Import declarations allow the inclusion of other .schema files to reuse definitions across multiple files.

#### Example
```plaintext
import "common.schema"
```

#### Grammar Rule
```antlrv4-tool
importDeclaration
: 'import' STRING_LITERAL NEWLINE+
;
```

### Schema Declaration
The core structure of the Schemata grammar, the schema declaration defines an entity or data model. Each schema contains fields with types and optional annotations.

#### Example
```plaintext
schema User {
id          UUID        @primaryKey
username    string      @unique
email       string?     @unique
createdAt   datetime    @default(now())
posts       Post[]      @relation
}
```

#### Grammar Rule
```antlrv4-tool
schemaDeclaration
: 'schema' ID '{' schemaBody '}' NEWLINE*
;

schemaBody
: (schemaFieldDeclaration)+
;
```

### Field Declaration
Fields are the individual elements within a schema, each with a name, type, optional attributes (such as primary key or uniqueness constraints), and optionally default values.

#### Example
```plaintext
id          UUID        @primaryKey
username    string      @unique
```

#### Grammar Rule
```antlrv4-tool
schemaFieldDeclaration
: annotations? ID typeExpression fieldAttributes? ('=' expression)? NEWLINE+
;
```

### Type Expression
Type expressions define the data type of a field. These types can be basic types, custom types, or collections of types. Types can also be nullable (denoted by a ? after the type).

#### Basic Types:
- int, string, boolean, float, double, datetime, date
#### Custom Types:
- User-defined types such as UUID or Post.
#### Collections:
- Collections of basic or custom types are denoted by [].
#### Nullable:
- Nullable fields are marked with a ?.

#### Example
```plaintext
email       string?     @unique
posts       Post[]      @relation
```

#### Grammar Rule
```antlrv4-tool
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
```

### Annotations
Annotations provide metadata about fields, such as whether a field is a primary key or if it has a default value. These annotations can be attached to fields or schema definitions.

#### Example
```plaintext
id          UUID        @primaryKey
createdAt   datetime    @default(now())
```

#### Grammar Rule
```antlrv4-tool
annotations
: annotation+
;

annotation
: '@' ID ('(' annotationParameters? ')')?
;

annotationParameters
: attributeParameter (',' attributeParameter)*
;

attributeParameter
: ID '=' literal
;
```

### Enum Declaration
Enums define a set of predefined values that a field can take. This is useful for representing limited, constant options such as user roles or statuses.

#### Example
```plaintext
enum UserStatus {
ACTIVE,
INACTIVE,
BANNED
}
```

#### Grammar Rule
```antlrv4-tool
enumDeclaration
: 'enum' ID '{' enumBody '}' NEWLINE*
;

enumBody
: (enumValue (NEWLINE | ',' | NEWLINE enumValue)*)? enumValue? NEWLINE*
;

enumValue
: ID
;
```

### Literals
Literals represent constant values such as strings, numbers, or boolean values. They can be used in field default values or annotations.

#### Example
```plaintext
@default("ACTIVE")
```

#### Grammar Rule
```antlrv4-tool
literal
: STRING_LITERAL
| NUMBER_LITERAL
| 'true'
| 'false'
;
```

### Qualified Names
Qualified names are used in namespace declarations or as references to other schemas or entities.

#### Example
```plaintext
namespace com.example.models
```

#### Grammar Rule
```antlrv4-tool
qualifiedName
: ID ('.' ID)*
;
```

### Additional Notes
- **Basic Types:** The fundamental types supported in the grammar include int, string, boolean, float, double, datetime, and date.

- **Custom Types:** User-defined types (e.g., UUID, Post) can be declared and used in schema definitions.

- **Nullable Fields:** A ? after a type signifies that the field is nullable, meaning it can have a null value.

- **Collections:** Collections of types (e.g., Post[]) are represented by appending [] to the type, allowing for lists or arrays of that type.

- **Annotations:** Provide metadata such as constraints (@unique, @primaryKey) or defaults (@default("value")).