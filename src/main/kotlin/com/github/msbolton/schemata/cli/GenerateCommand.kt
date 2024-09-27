package com.github.msbolton.schemata.cli

import com.github.msbolton.schemata.generation.CodeGenerator
import com.github.msbolton.schemata.generation.java.JavaCodeGenerator
import com.github.msbolton.schemata.parsing.SchemaParserUtil
import com.github.msbolton.schemata.parsing.SchemataVisitorImpl
import picocli.CommandLine
import picocli.CommandLine.Parameters
import java.io.File

@CommandLine.Command(
    name = "generate",
    description = ["Generates code from Schemata IDL"]
)
class GenerateCommand : Runnable {

    @Parameters(index = "0", description = ["The path to the Schemata file"])
    lateinit var schemaFile: File

    @Parameters(index = "1", description = ["The output directory"])
    lateinit var outputDir: File

    @CommandLine.Option(names = ["-l", "--language"], description = ["The target language"], required = true)
    lateinit var lang: String

    override fun run() {
        // Parse the schema file
        val parseTree = SchemaParserUtil.parseSchemaFile(schemaFile)
        // Visit parse tree to extract class model
        val visitor = SchemataVisitorImpl()
        val classModel = visitor.visit(parseTree)

        val generator: CodeGenerator = when (lang.lowercase()) {
            "java" -> JavaCodeGenerator()
            else -> throw IllegalArgumentException("Unsupported language: $lang")
        }

        if (classModel != null) {
            generator.generate(classModel, outputDir)
        }
        println("Code generation for $lang completed.")
    }
}
