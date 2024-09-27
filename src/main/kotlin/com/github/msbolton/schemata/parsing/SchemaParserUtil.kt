package com.github.msbolton.schemata.parsing

import org.antlr.v4.runtime.CharStreams
import org.antlr.v4.runtime.CommonTokenStream
import java.io.File

object SchemaParserUtil {
    fun parseSchemaFile(file: File): SchemataParser.SchemataContext {
        val input = CharStreams.fromFileName(file.absolutePath)
        val lexer = SchemataLexer(input)
        val tokens = CommonTokenStream(lexer)
        val parser = SchemataParser(tokens)

        return parser.schemata()
    }
}