package com.github.msbolton.schemata.generation.java

import com.github.msbolton.schemata.generation.CodeGenerator
import com.github.msbolton.schemata.models.ClassModel
import com.github.msbolton.schemata.templates.TemplateManager
import java.io.File

class JavaCodeGenerator : CodeGenerator {

    override fun generate(classModel: ClassModel, outputDir: File): File {
        val template = TemplateManager.loadTemplate("java/class_template.ftl")

        val outputFile = File(outputDir, "${classModel.className}.java")
        outputFile.writeText(TemplateManager.processTemplate(template, classModel))

        return outputFile
    }
}