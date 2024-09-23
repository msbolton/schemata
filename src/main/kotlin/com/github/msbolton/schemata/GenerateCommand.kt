package com.github.msbolton.schemata

import picocli.CommandLine

@CommandLine.Command(
    name = "generate",
    description = ["Generates code from Schemata IDL files"]
)
class GenerateCommand : Runnable {

    @CommandLine.Option(names = ["-i", "--input"], description = ["Input .schema file"], required = true)
    lateinit var inputFile: String

    @CommandLine.Option(names = ["-o", "--output"], description = ["Output directory"], required = true)
    lateinit var outputDir: String

    override fun run() {
        // Implement code generation logic
        // Parse the input .schema file
        // Generate code in the specified output directory
    }
}
