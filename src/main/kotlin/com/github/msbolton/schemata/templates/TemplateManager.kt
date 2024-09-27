package com.github.msbolton.schemata.templates

import freemarker.template.Configuration
import freemarker.template.Template
import java.io.StringWriter

object TemplateManager {

    private val config: Configuration = Configuration(Configuration.VERSION_2_3_31).apply {
        setClassLoaderForTemplateLoading(Thread.currentThread().contextClassLoader, "templates")
        defaultEncoding = "UTF-8"
    }

    fun loadTemplate(templatePath: String): Template {
        return config.getTemplate(templatePath)
    }

    fun processTemplate(template: Template, dataModel: Any): String {
        val writer = StringWriter()
        template.process(dataModel, writer)
        return writer.toString()
    }
}